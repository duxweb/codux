use crate::ai_runtime::{
    AIHookEventMetadata, runtime_event_dir, runtime_frame_to_hook, runtime_state_for_hook_kind,
    status_for_runtime_state,
};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEventSummary {
    pub event_dir: String,
    pub file_count: usize,
    pub decoded_count: usize,
    pub failed_count: usize,
    pub running_count: usize,
    pub needs_input_count: usize,
    pub completed_count: usize,
    pub by_tool: Vec<RuntimeEventCount>,
    pub by_kind: Vec<RuntimeEventCount>,
    pub sessions: Vec<RuntimeSessionSummary>,
    pub recent_events: Vec<RuntimeEventItem>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEventCount {
    pub label: String,
    pub count: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeEventItem {
    pub file_name: String,
    pub tool: String,
    pub kind: String,
    pub state: String,
    pub project_name: String,
    pub terminal_id: String,
    pub session_title: String,
    pub updated_at: f64,
    pub modified_at: f64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeSessionSummary {
    pub terminal_id: String,
    pub tool: String,
    pub state: String,
    pub project_name: String,
    pub session_title: String,
    pub updated_at: f64,
    pub event_count: usize,
}

pub struct RuntimeEventService {
    event_dir: PathBuf,
}

impl RuntimeEventService {
    pub fn new() -> Self {
        Self {
            event_dir: runtime_event_dir(),
        }
    }

    pub fn summary(&self) -> RuntimeEventSummary {
        let mut summary = RuntimeEventSummary {
            event_dir: self.event_dir.display().to_string(),
            ..Default::default()
        };
        let Ok(entries) = fs::read_dir(&self.event_dir) else {
            summary.error = Some("runtime event directory not found".to_string());
            return summary;
        };

        let mut by_tool = BTreeMap::<String, usize>::new();
        let mut by_kind = BTreeMap::<String, usize>::new();
        let mut sessions = BTreeMap::<String, RuntimeSessionSummary>::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            summary.file_count += 1;
            match decode_runtime_event_file(&path) {
                Some(event) => {
                    summary.decoded_count += 1;
                    *by_tool.entry(event.tool.clone()).or_default() += 1;
                    *by_kind.entry(event.kind.clone()).or_default() += 1;
                    merge_session_event(&mut sessions, &event);
                    summary.recent_events.push(event);
                }
                None => summary.failed_count += 1,
            }
        }

        summary
            .recent_events
            .sort_by(|left, right| right.modified_at.total_cmp(&left.modified_at));
        summary.recent_events.truncate(10);
        summary.sessions = sessions.into_values().collect::<Vec<_>>();
        summary
            .sessions
            .sort_by(|left, right| right.updated_at.total_cmp(&left.updated_at));
        summary.running_count = summary
            .sessions
            .iter()
            .filter(|session| session.state == "running")
            .count();
        summary.needs_input_count = summary
            .sessions
            .iter()
            .filter(|session| session.state == "needs-input")
            .count();
        summary.completed_count = summary
            .sessions
            .iter()
            .filter(|session| session.state == "completed")
            .count();
        summary.by_tool = count_rows(by_tool);
        summary.by_kind = count_rows(by_kind);
        summary
    }
}

fn decode_runtime_event_file(path: &Path) -> Option<RuntimeEventItem> {
    let data = fs::read(path).ok()?;
    let data = data.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&data);
    let modified_at = path
        .metadata()
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0);

    let payload = runtime_frame_to_hook(data)?;
    let runtime_state = runtime_state_for_hook_kind(&payload.kind, payload.metadata.as_ref());
    let mut item = RuntimeEventItem {
        file_name: file_name(path),
        tool: payload.tool,
        state: event_summary_state(&payload.kind, payload.metadata.as_ref(), runtime_state)
            .to_string(),
        kind: payload.kind,
        project_name: payload.project_name,
        terminal_id: payload.terminal_id,
        session_title: payload.session_title,
        updated_at: payload.updated_at,
        modified_at,
    };
    if item.tool.trim().is_empty() {
        item.tool = "unknown".to_string();
    }
    if item.kind.trim().is_empty() {
        item.kind = "unknown".to_string();
    }
    Some(item)
}

fn event_summary_state(
    kind: &str,
    metadata: Option<&AIHookEventMetadata>,
    runtime_state: &str,
) -> &'static str {
    match kind {
        "turnCompleted" | "sessionEnded" => "completed",
        "needsInput" => "needs-input",
        _ if metadata
            .and_then(|metadata| metadata.notification_type.as_deref())
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false) =>
        {
            "needs-input"
        }
        _ => status_for_runtime_state(runtime_state),
    }
}

fn merge_session_event(
    sessions: &mut BTreeMap<String, RuntimeSessionSummary>,
    event: &RuntimeEventItem,
) {
    let entry = sessions
        .entry(event.terminal_id.clone())
        .or_insert_with(|| RuntimeSessionSummary {
            terminal_id: event.terminal_id.clone(),
            tool: event.tool.clone(),
            state: event.state.clone(),
            project_name: event.project_name.clone(),
            session_title: event.session_title.clone(),
            updated_at: event.updated_at,
            event_count: 0,
        });
    entry.event_count += 1;
    if event.updated_at >= entry.updated_at {
        entry.tool = event.tool.clone();
        entry.state = event.state.clone();
        entry.project_name = event.project_name.clone();
        entry.session_title = event.session_title.clone();
        entry.updated_at = event.updated_at;
    }
}

fn count_rows(values: BTreeMap<String, usize>) -> Vec<RuntimeEventCount> {
    let mut rows = values
        .into_iter()
        .map(|(label, count)| RuntimeEventCount { label, count })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.label.cmp(&right.label))
    });
    rows
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn summary_decodes_ai_hook_and_opencode_runtime_events() {
        let dir =
            std::env::temp_dir().join(format!("codux-gpui-runtime-events-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("hook.json"),
            r#"{
              "kind": "ai-hook",
              "payload": {
                "kind": "promptSubmitted",
                "terminalID": "term-1",
                "projectID": "project-1",
                "projectName": "Codux",
                "sessionTitle": "Build",
                "tool": "codex",
                "updatedAt": 10
              }
            }"#,
        )
        .unwrap();
        fs::write(
            dir.join("opencode.json"),
            r#"{
              "kind": "opencode-runtime",
              "payload": {
                "sessionId": "term-2",
                "projectId": "project-1",
                "projectName": "Codux",
                "sessionTitle": "Review",
                "tool": "opencode",
                "status": "completed",
                "responseState": "idle",
                "updatedAt": 20
              }
            }"#,
        )
        .unwrap();
        fs::write(dir.join("bad.json"), "{").unwrap();

        let summary = RuntimeEventService {
            event_dir: dir.clone(),
        }
        .summary();

        assert_eq!(summary.file_count, 3);
        assert_eq!(summary.decoded_count, 2);
        assert_eq!(summary.failed_count, 1);
        assert_eq!(summary.running_count, 1);
        assert_eq!(summary.completed_count, 1);
        assert_eq!(summary.sessions.len(), 2);
        assert!(
            summary
                .by_tool
                .iter()
                .any(|row| row.label == "codex" && row.count == 1)
        );
        assert!(
            summary
                .sessions
                .iter()
                .any(|session| session.terminal_id == "term-1" && session.state == "running")
        );
        assert!(
            summary
                .by_kind
                .iter()
                .any(|row| row.label == "turnCompleted" && row.count == 1)
        );

        fs::remove_dir_all(dir).unwrap();
    }
}
