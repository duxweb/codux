use crate::ai_runtime::{
    probe::{
        common::{json_i64, parse_iso8601_seconds},
        paths::{directory_files, file_modified_millis, paths_equivalent},
    },
    snapshot::{AIRuntimeContextSnapshot, AIRuntimeProbeRequest},
    state::normalized_string,
};
use serde_json::Value;
use std::{fs, path::Path, path::PathBuf};

pub(crate) fn probe_codewhale_runtime(
    request: &AIRuntimeProbeRequest,
) -> Option<AIRuntimeContextSnapshot> {
    let project_path = normalized_string(request.project_path.as_deref())?;
    let preferred_id = normalized_string(request.external_session_id.as_deref());
    let session_files = normalized_string(request.transcript_path.as_deref())
        .map(PathBuf::from)
        .map(|path| vec![path])
        .unwrap_or_else(|| {
            directory_files(
                &crate::runtime_paths::home_dir()
                    .join(".codewhale")
                    .join("sessions"),
                "json",
            )
        });
    let mut states = session_files
        .into_iter()
        .filter_map(|path| parse_codewhale_runtime_state(&path, &project_path))
        .collect::<Vec<_>>();
    states.sort_by(|left, right| right.updated_at.total_cmp(&left.updated_at));

    let mut preferred_match = None;
    let mut current_launch_match = None;
    let mut candidate_match = None;
    for state in states.into_iter().take(16) {
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
                .map(|existing: &CodeWhaleParsedState| state.updated_at > existing.updated_at)
                .unwrap_or(true)
            {
                current_launch_match = Some(state.clone());
            }
            continue;
        }
        if candidate_match
            .as_ref()
            .map(|existing: &CodeWhaleParsedState| state.updated_at > existing.updated_at)
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

    // CodeWhale's live running/completed state is owned by the OSC progress hook
    // (see CodeWhaleTerminalProgressWatcher / submit_progress_hook): the
    // `codux-message-submit` hook marks the turn responding and an OSC
    // "completed" sequence ends it. The session file is a rest snapshot with no
    // live state field, so for a session started THIS launch ("fresh") the probe
    // stays neutral (no response_state) and lets the OSC signal own the state.
    // Asserting idle+completed here would demote the live turn on every 5s poll
    // -- and because the OSC completion only fires while the session is still
    // "running", that false demotion also suppressed the real turn-complete. A
    // restored session (no OSC events this launch) is reported as finished so it
    // renders as completed.
    let restored = state.origin == "restored";
    Some(AIRuntimeContextSnapshot {
        tool: "codewhale".to_string(),
        external_session_id: Some(state.external_session_id),
        transcript_path: Some(state.file_path),
        model: state.model,
        assistant_preview: state.assistant_preview,
        input_tokens: state.total_tokens,
        output_tokens: 0,
        cached_input_tokens: 0,
        total_tokens: state.total_tokens,
        updated_at: state.updated_at.max(request.updated_at),
        started_at: Some(state.started_at),
        completed_at: restored.then_some(state.updated_at),
        response_state: restored.then(|| "idle".to_string()),
        was_interrupted: false,
        has_completed_turn: restored,
        session_origin: state.origin,
        source: "probe".to_string(),
        plan: None,
    })
}

#[derive(Clone)]
struct CodeWhaleParsedState {
    external_session_id: String,
    file_path: String,
    model: Option<String>,
    assistant_preview: Option<String>,
    total_tokens: i64,
    started_at: f64,
    updated_at: f64,
    origin: String,
}

fn parse_codewhale_runtime_state(
    file_path: &Path,
    project_path: &str,
) -> Option<CodeWhaleParsedState> {
    let data = fs::read_to_string(file_path).ok()?;
    let value = serde_json::from_str::<Value>(&data).ok()?;
    let metadata = value.get("metadata").unwrap_or(&Value::Null);
    let workspace = metadata
        .get("workspace")
        .or_else(|| value.get("workspace"))
        .and_then(|value| value.as_str())?;
    if !paths_equivalent(Some(workspace), project_path) {
        return None;
    }

    let external_session_id = metadata
        .get("id")
        .or_else(|| value.get("id"))
        .and_then(|value| value.as_str())
        .and_then(|value| normalized_string(Some(value)))
        .or_else(|| {
            file_path
                .file_stem()
                .and_then(|value| value.to_str())
                .and_then(|value| normalized_string(Some(value)))
        })?;
    let started_at = metadata
        .get("created_at")
        .or_else(|| metadata.get("createdAt"))
        .and_then(|value| value.as_str())
        .and_then(parse_iso8601_seconds)
        .unwrap_or(0.0);
    let updated_at = metadata
        .get("updated_at")
        .or_else(|| metadata.get("updatedAt"))
        .and_then(|value| value.as_str())
        .and_then(parse_iso8601_seconds)
        .unwrap_or_else(|| {
            file_modified_millis(file_path)
                .map(|value| value as f64 / 1000.0)
                .unwrap_or(started_at)
        });
    let model = metadata
        .get("model")
        .or_else(|| value.get("model"))
        .and_then(|value| value.as_str())
        .and_then(|value| normalized_string(Some(value)));
    let total_tokens = json_i64(
        metadata
            .get("total_tokens")
            .or_else(|| metadata.get("totalTokens"))
            .or_else(|| value.get("total_tokens"))
            .or_else(|| value.get("totalTokens")),
    );

    Some(CodeWhaleParsedState {
        external_session_id,
        file_path: file_path.display().to_string(),
        model,
        assistant_preview: latest_assistant_preview(&value),
        total_tokens,
        started_at,
        updated_at,
        origin: "restored".to_string(),
    })
}

fn latest_assistant_preview(value: &Value) -> Option<String> {
    value
        .get("messages")
        .and_then(|value| value.as_array())
        .and_then(|messages| {
            messages.iter().rev().find_map(|message| {
                if message.get("role").and_then(|value| value.as_str()) != Some("assistant") {
                    return None;
                }
                message_text(message.get("content").unwrap_or(&Value::Null))
            })
        })
}

fn message_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => normalized_string(Some(text)),
        Value::Array(items) => items.iter().find_map(message_text),
        Value::Object(object) => object
            .get("text")
            .or_else(|| object.get("content"))
            .and_then(message_text),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn parses_codewhale_runtime_state_for_project() {
        let root = std::env::temp_dir().join(format!("codux-codewhale-probe-{}", Uuid::new_v4()));
        let project = root.join("project");
        let session_dir = root.join(".codewhale").join("sessions");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&session_dir).unwrap();
        let session_file = session_dir.join("session-1.json");
        fs::write(
            &session_file,
            format!(
                r#"{{
                    "metadata": {{
                        "id": "session-1",
                        "workspace": "{}",
                        "model": "deepseek-chat",
                        "total_tokens": 345,
                        "created_at": "2026-06-06T01:00:00Z",
                        "updated_at": "2026-06-06T01:01:00Z"
                    }},
                    "messages": [
                        {{ "role": "user", "content": "hi" }},
                        {{ "role": "assistant", "content": [{{ "type": "text", "text": "hello" }}] }}
                    ]
                }}"#,
                project.display()
            ),
        )
        .unwrap();

        let parsed =
            parse_codewhale_runtime_state(&session_file, project.to_str().unwrap()).unwrap();

        assert_eq!(parsed.external_session_id, "session-1");
        assert_eq!(parsed.model.as_deref(), Some("deepseek-chat"));
        assert_eq!(parsed.total_tokens, 345);
        assert_eq!(parsed.assistant_preview.as_deref(), Some("hello"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn ignores_codewhale_runtime_state_for_other_project() {
        let root = std::env::temp_dir().join(format!("codux-codewhale-probe-{}", Uuid::new_v4()));
        let session_dir = root.join(".codewhale").join("sessions");
        fs::create_dir_all(&session_dir).unwrap();
        let session_file = session_dir.join("session-1.json");
        fs::write(
            &session_file,
            r#"{
                "metadata": {
                    "id": "session-1",
                    "workspace": "/tmp/other-project",
                    "model": "deepseek-chat",
                    "total_tokens": 345
                }
            }"#,
        )
        .unwrap();

        let parsed = parse_codewhale_runtime_state(&session_file, "/tmp/project");

        assert!(parsed.is_none());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn probes_codewhale_runtime_from_transcript_path() {
        let root = std::env::temp_dir().join(format!("codux-codewhale-probe-{}", Uuid::new_v4()));
        let project = root.join("project");
        let session_dir = root.join(".codewhale").join("sessions");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&session_dir).unwrap();
        let session_file = session_dir.join("session-1.json");
        fs::write(
            &session_file,
            format!(
                r#"{{
                    "metadata": {{
                        "id": "session-1",
                        "workspace": "{}",
                        "model": "deepseek-reasoner",
                        "total_tokens": 456,
                        "created_at": "2026-06-06T01:00:00Z",
                        "updated_at": "2026-06-06T01:01:00Z"
                    }}
                }}"#,
                project.display()
            ),
        )
        .unwrap();

        let snapshot = probe_codewhale_runtime(&AIRuntimeProbeRequest {
            terminal_id: "terminal-1".to_string(),
            terminal_instance_id: Some("instance-1".to_string()),
            project_id: "project-1".to_string(),
            project_path: Some(project.display().to_string()),
            tool: "codewhale".to_string(),
            external_session_id: Some("session-1".to_string()),
            transcript_path: Some(session_file.display().to_string()),
            started_at: Some(0.0),
            updated_at: 1.0,
        })
        .unwrap();

        assert_eq!(snapshot.tool, "codewhale");
        assert_eq!(snapshot.external_session_id.as_deref(), Some("session-1"));
        assert_eq!(snapshot.model.as_deref(), Some("deepseek-reasoner"));
        assert_eq!(snapshot.total_tokens, 456);
        // Started this launch (request.started_at = 0 <= session start) -> fresh,
        // so the probe defers to the live OSC progress signal instead of
        // asserting a completed turn that would demote the running state.
        assert_eq!(snapshot.response_state, None);
        assert!(!snapshot.has_completed_turn);
        assert_eq!(snapshot.completed_at, None);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn restored_codewhale_session_reports_completed() {
        let root = std::env::temp_dir().join(format!("codux-codewhale-probe-{}", Uuid::new_v4()));
        let project = root.join("project");
        let session_dir = root.join(".codewhale").join("sessions");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&session_dir).unwrap();
        let session_file = session_dir.join("session-1.json");
        fs::write(
            &session_file,
            format!(
                r#"{{
                    "metadata": {{
                        "id": "session-1",
                        "workspace": "{}",
                        "model": "deepseek-reasoner",
                        "total_tokens": 456,
                        "created_at": "2026-06-06T01:00:00Z",
                        "updated_at": "2026-06-06T01:01:00Z"
                    }}
                }}"#,
                project.display()
            ),
        )
        .unwrap();

        // request.started_at far in the future -> the session predates this
        // launch (restored), so it is reported as a finished turn (completed).
        let snapshot = probe_codewhale_runtime(&AIRuntimeProbeRequest {
            terminal_id: "terminal-1".to_string(),
            terminal_instance_id: Some("instance-1".to_string()),
            project_id: "project-1".to_string(),
            project_path: Some(project.display().to_string()),
            tool: "codewhale".to_string(),
            external_session_id: Some("session-1".to_string()),
            transcript_path: Some(session_file.display().to_string()),
            started_at: Some(9_999_999_999.0),
            updated_at: 1.0,
        })
        .unwrap();

        assert_eq!(snapshot.response_state.as_deref(), Some("idle"));
        assert!(snapshot.has_completed_turn);

        let _ = fs::remove_dir_all(root);
    }
}
