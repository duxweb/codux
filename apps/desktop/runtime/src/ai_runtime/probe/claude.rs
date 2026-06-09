use crate::ai_runtime::{
    probe::{
        common::{
            first_object_deep, first_string_deep, json_i64, now_seconds, parse_iso8601_seconds,
        },
        paths::{claude_project_log_paths, paths_equivalent},
        preview::sanitized_preview_from_values,
    },
    snapshot::{AIPlanItem, AIPlanSnapshot, AIRuntimeContextSnapshot, AIRuntimeProbeRequest},
    state::normalized_string,
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader},
    path::Path,
};

pub(crate) fn probe_claude_runtime(
    request: &AIRuntimeProbeRequest,
) -> Option<AIRuntimeContextSnapshot> {
    let project_path = normalized_string(request.project_path.as_deref())?;
    let external_id = normalized_string(request.external_session_id.as_deref())?;
    let file_urls = claude_project_log_paths(&project_path);
    let mut aggregate: Option<ClaudeAggregate> = None;
    for file_url in file_urls {
        let Some(next) = parse_claude_log_runtime_state(&file_url, &project_path, &external_id)
        else {
            continue;
        };
        aggregate = Some(match aggregate {
            Some(existing) => existing.merge(next),
            None => next,
        });
    }
    let aggregate = aggregate?;
    let plan = aggregate.plan(&external_id);
    let started_at = aggregate.started_at();
    let completed_at = aggregate.completed_at();
    let response_state = aggregate.response_state();
    let was_interrupted = aggregate.was_interrupted();
    let has_completed_turn = aggregate.has_completed_turn();
    Some(AIRuntimeContextSnapshot {
        tool: "claude".to_string(),
        external_session_id: Some(external_id),
        transcript_path: None,
        model: aggregate.model,
        assistant_preview: aggregate.assistant_preview,
        input_tokens: aggregate.input_tokens,
        output_tokens: aggregate.output_tokens,
        cached_input_tokens: aggregate.cached_input_tokens,
        total_tokens: aggregate.total_tokens,
        updated_at: aggregate.updated_at.max(request.updated_at),
        started_at,
        completed_at,
        response_state,
        was_interrupted,
        has_completed_turn,
        session_origin: "unknown".to_string(),
        source: "probe".to_string(),
        plan,
    })
}

#[derive(Default)]
struct ClaudeAggregate {
    model: Option<String>,
    assistant_preview: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
    cached_input_tokens: i64,
    total_tokens: i64,
    updated_at: f64,
    last_user_at: f64,
    last_completion_at: f64,
    last_interrupted_at: f64,
    last_completed_turn_at: f64,
    tasks: BTreeMap<String, AIPlanItem>,
    task_list: Option<Vec<AIPlanItem>>,
    task_updated_at: f64,
}

impl ClaudeAggregate {
    fn merge(self, other: Self) -> Self {
        Self {
            model: other.model.or(self.model),
            assistant_preview: other.assistant_preview.or(self.assistant_preview),
            input_tokens: self.input_tokens + other.input_tokens,
            output_tokens: self.output_tokens + other.output_tokens,
            cached_input_tokens: self.cached_input_tokens + other.cached_input_tokens,
            total_tokens: self.total_tokens + other.total_tokens,
            updated_at: self.updated_at.max(other.updated_at),
            last_user_at: self.last_user_at.max(other.last_user_at),
            last_completion_at: self.last_completion_at.max(other.last_completion_at),
            last_interrupted_at: self.last_interrupted_at.max(other.last_interrupted_at),
            last_completed_turn_at: self
                .last_completed_turn_at
                .max(other.last_completed_turn_at),
            tasks: merge_claude_tasks(self.tasks, other.tasks),
            task_list: other.task_list.or(self.task_list),
            task_updated_at: self.task_updated_at.max(other.task_updated_at),
        }
    }

    fn plan(&self, session_id: &str) -> Option<AIPlanSnapshot> {
        let items = self
            .task_list
            .clone()
            .unwrap_or_else(|| self.tasks.values().cloned().collect());
        (!items.is_empty()).then_some(AIPlanSnapshot {
            source: "claude".to_string(),
            session_id: session_id.to_string(),
            updated_at: self.task_updated_at.max(self.updated_at),
            items,
        })
    }

    fn started_at(&self) -> Option<f64> {
        (self.last_user_at > 0.0).then_some(self.last_user_at)
    }

    fn completed_at(&self) -> Option<f64> {
        let completion = self.last_completed_turn_at.max(self.last_interrupted_at);
        (completion > 0.0).then_some(completion)
    }

    fn response_state(&self) -> Option<String> {
        if self.last_user_at <= 0.0 {
            return None;
        }
        if self.last_user_at > self.last_completion_at {
            Some("responding".to_string())
        } else {
            Some("idle".to_string())
        }
    }

    fn was_interrupted(&self) -> bool {
        if self.last_interrupted_at <= 0.0 {
            return false;
        }
        let latest_conflicting_at = self.last_user_at.max(self.last_completed_turn_at);
        self.last_interrupted_at >= latest_conflicting_at
    }

    fn has_completed_turn(&self) -> bool {
        if self.last_completed_turn_at <= 0.0 {
            return false;
        }
        let latest_conflicting_at = self.last_user_at.max(self.last_interrupted_at);
        self.last_completed_turn_at >= latest_conflicting_at
    }
}

fn parse_claude_log_runtime_state(
    file_path: &Path,
    project_path: &str,
    external_session_id: &str,
) -> Option<ClaudeAggregate> {
    let file = fs::File::open(file_path).ok()?;
    let reader = BufReader::new(file);
    let mut aggregate = ClaudeAggregate::default();
    let mut matched = false;

    for line in reader.lines().map_while(Result::ok) {
        let Ok(row) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if row.get("sessionId").and_then(|value| value.as_str()) != Some(external_session_id) {
            continue;
        }
        if let Some(cwd) = row.get("cwd").and_then(|value| value.as_str()) {
            if !paths_equivalent(Some(cwd), project_path) {
                continue;
            }
        }
        matched = true;
        let timestamp = row
            .get("timestamp")
            .and_then(|value| value.as_str())
            .and_then(parse_iso8601_seconds)
            .unwrap_or_else(now_seconds);
        aggregate.updated_at = aggregate.updated_at.max(timestamp);
        let message = row.get("message").unwrap_or(&Value::Null);
        let role = message
            .get("role")
            .and_then(|value| value.as_str())
            .or_else(|| row.get("type").and_then(|value| value.as_str()));
        if role == Some("user") {
            if is_claude_interrupted_row(&row) {
                aggregate.last_interrupted_at = aggregate.last_interrupted_at.max(timestamp);
                aggregate.last_completion_at = aggregate.last_completion_at.max(timestamp);
            } else {
                aggregate.last_user_at = aggregate.last_user_at.max(timestamp);
            }
        } else if role == Some("assistant") {
            let stop_reason = message.get("stop_reason").and_then(|value| value.as_str());
            if stop_reason == Some("end_turn") {
                aggregate.last_completed_turn_at = aggregate.last_completed_turn_at.max(timestamp);
                aggregate.last_completion_at = aggregate.last_completion_at.max(timestamp);
            }
            if let Some(preview) =
                sanitized_preview_from_values(&[message.get("content"), row.get("content")])
            {
                aggregate.assistant_preview = Some(preview);
            }
            parse_claude_task_tool_uses(message, timestamp, &mut aggregate);
        } else if role == Some("system") {
            let subtype = row.get("subtype").and_then(|value| value.as_str());
            if matches!(subtype, Some("turn_duration" | "stop_hook_summary")) {
                aggregate.last_completion_at = aggregate.last_completion_at.max(timestamp);
            }
        }
        parse_claude_task_result(&row, timestamp, &mut aggregate);
        if let Some(model) = first_string_deep(&row, &["model"]) {
            aggregate.model = Some(model);
        }
        if let Some(usage) = first_object_deep(&row, &["usage"]) {
            aggregate.input_tokens += json_i64(usage.get("input_tokens"));
            aggregate.output_tokens += json_i64(usage.get("output_tokens"));
            aggregate.cached_input_tokens += json_i64(usage.get("cache_creation_input_tokens"))
                + json_i64(usage.get("cache_read_input_tokens"));
            aggregate.total_tokens +=
                json_i64(usage.get("input_tokens")) + json_i64(usage.get("output_tokens"));
        }
    }

    if !matched {
        return None;
    }
    Some(aggregate)
}

fn merge_claude_tasks(
    mut left: BTreeMap<String, AIPlanItem>,
    right: BTreeMap<String, AIPlanItem>,
) -> BTreeMap<String, AIPlanItem> {
    for (key, value) in right {
        left.insert(key, value);
    }
    left
}

fn parse_claude_task_tool_uses(
    message: &Value,
    timestamp: f64,
    aggregate: &mut ClaudeAggregate,
) {
    let Some(content) = message.get("content").and_then(|value| value.as_array()) else {
        return;
    };
    for item in content {
        if item.get("type").and_then(|value| value.as_str()) != Some("tool_use") {
            continue;
        }
        match item.get("name").and_then(|value| value.as_str()) {
            Some("TaskCreate") => parse_claude_task_create(item, timestamp, aggregate),
            Some("TaskUpdate") => parse_claude_task_update(item, timestamp, aggregate),
            _ => {}
        }
    }
}

fn parse_claude_task_create(item: &Value, timestamp: f64, aggregate: &mut ClaudeAggregate) {
    let input = item.get("input").unwrap_or(&Value::Null);
    let Some(text) = input
        .get("subject")
        .and_then(|value| value.as_str())
        .or_else(|| input.get("description").and_then(|value| value.as_str()))
        .and_then(|value| normalized_string(Some(value)))
    else {
        return;
    };
    let key = input
        .get("id")
        .and_then(|value| value.as_str())
        .or_else(|| item.get("id").and_then(|value| value.as_str()))
        .and_then(|value| normalized_string(Some(value)))
        .unwrap_or_else(|| format!("pending-{}", aggregate.tasks.len() + 1));
    aggregate.tasks.insert(
        key,
        AIPlanItem {
            text,
            status: "pending".to_string(),
            priority: None,
        },
    );
    aggregate.task_updated_at = aggregate.task_updated_at.max(timestamp);
}

fn parse_claude_task_update(item: &Value, timestamp: f64, aggregate: &mut ClaudeAggregate) {
    let input = item.get("input").unwrap_or(&Value::Null);
    let Some(task_id) = input
        .get("taskId")
        .and_then(|value| value.as_str())
        .or_else(|| input.get("id").and_then(|value| value.as_str()))
        .or_else(|| item.get("id").and_then(|value| value.as_str()))
        .and_then(|value| normalized_string(Some(value)))
    else {
        return;
    };
    let status = input
        .get("status")
        .and_then(|value| value.as_str())
        .map(normalized_plan_status)
        .unwrap_or_else(|| "pending".to_string());
    aggregate
        .tasks
        .entry(task_id)
        .and_modify(|task| task.status = status.clone())
        .or_insert(AIPlanItem {
            text: "Task".to_string(),
            status,
            priority: None,
        });
    aggregate.task_updated_at = aggregate.task_updated_at.max(timestamp);
}

fn parse_claude_task_result(row: &Value, timestamp: f64, aggregate: &mut ClaudeAggregate) {
    if let Some(tasks) = row
        .get("toolUseResult")
        .and_then(|value| value.get("tasks"))
        .and_then(|value| value.as_array())
    {
        let items = tasks
            .iter()
            .filter_map(|task| {
                let text = task
                    .get("subject")
                    .and_then(|value| value.as_str())
                    .and_then(|value| normalized_string(Some(value)))?;
                let status = task
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
        if !items.is_empty() {
            aggregate.task_list = Some(items);
            aggregate.task_updated_at = aggregate.task_updated_at.max(timestamp);
        }
        return;
    }

    let Some(task) = row
        .get("toolUseResult")
        .and_then(|value| value.get("task"))
        .and_then(|value| value.as_object())
    else {
        return;
    };
    let Some(id) = task
        .get("id")
        .and_then(|value| value.as_str())
        .and_then(|value| normalized_string(Some(value)))
    else {
        return;
    };
    let Some(subject) = task
        .get("subject")
        .and_then(|value| value.as_str())
        .and_then(|value| normalized_string(Some(value)))
    else {
        return;
    };
    aggregate.tasks.insert(
        id,
        AIPlanItem {
            text: subject,
            status: "pending".to_string(),
            priority: None,
        },
    );
    aggregate.task_updated_at = aggregate.task_updated_at.max(timestamp);
}

fn normalized_plan_status(value: &str) -> String {
    match value.trim() {
        "completed" | "complete" | "done" => "completed",
        "in_progress" | "in-progress" | "running" | "active" => "in_progress",
        _ => "pending",
    }
    .to_string()
}

fn is_claude_interrupted_row(row: &Value) -> bool {
    let text = row.to_string().to_lowercase();
    text.contains("interrupted") || text.contains("cancelled") || text.contains("aborted")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_task_list_result_into_plan() {
        let mut aggregate = ClaudeAggregate::default();
        let row = serde_json::json!({
            "toolUseResult": {
                "tasks": [
                    {"id": "a", "subject": "Inspect logs", "status": "completed"},
                    {"id": "b", "subject": "Patch parser", "status": "in_progress"}
                ]
            }
        });

        parse_claude_task_result(&row, 42.0, &mut aggregate);
        let plan = aggregate.plan("claude-session").expect("plan");

        assert_eq!(plan.source, "claude");
        assert_eq!(plan.session_id, "claude-session");
        assert_eq!(plan.updated_at, 42.0);
        assert_eq!(plan.items.len(), 2);
        assert_eq!(plan.items[0].text, "Inspect logs");
        assert_eq!(plan.items[0].status, "completed");
        assert_eq!(plan.items[1].status, "in_progress");
    }

    #[test]
    fn updates_created_task_status() {
        let mut aggregate = ClaudeAggregate::default();
        let create = serde_json::json!({
            "type": "tool_use",
            "id": "tool-a",
            "name": "TaskCreate",
            "input": {"id": "task-a", "subject": "Write tests"}
        });
        let update = serde_json::json!({
            "type": "tool_use",
            "name": "TaskUpdate",
            "input": {"taskId": "task-a", "status": "completed"}
        });

        parse_claude_task_create(&create, 10.0, &mut aggregate);
        parse_claude_task_update(&update, 11.0, &mut aggregate);
        let plan = aggregate.plan("claude-session").expect("plan");

        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].text, "Write tests");
        assert_eq!(plan.items[0].status, "completed");
        assert_eq!(plan.updated_at, 11.0);
    }
}
