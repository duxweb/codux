use crate::ai_runtime::{
    probe::{
        common::{
            first_object_deep, first_string_deep, is_awaiting_user_decision, json_i64, now_seconds,
            parse_iso8601_seconds,
        },
        paths::{claude_project_log_paths, newest_claude_session_id_since, paths_equivalent},
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
    // Unknown session id → derive it from the freshest transcript for this cwd.
    let external_id = normalized_string(request.external_session_id.as_deref())
        .or_else(|| newest_claude_session_id_since(&project_path, request.started_at))?;
    let file_urls = claude_project_log_paths(&project_path);
    let mut aggregate: Option<ClaudeAggregate> = None;
    // Track the matched file with the most recent activity so the supervisor can
    // size+mtime watch it (parity with codex); claude sessions are otherwise
    // only re-probed on the 5s interval.
    let mut transcript_path: Option<String> = None;
    let mut transcript_seen_at = f64::MIN;
    for file_url in file_urls {
        let Some(next) = parse_claude_log_runtime_state(&file_url, &project_path, &external_id)
        else {
            continue;
        };
        if next.updated_at >= transcript_seen_at {
            transcript_seen_at = next.updated_at;
            transcript_path = Some(file_url.display().to_string());
        }
        aggregate = Some(match aggregate {
            Some(existing) => existing.merge(next),
            None => next,
        });
    }
    let aggregate = aggregate?;
    let plan = aggregate.plan(&external_id);
    let started_at = aggregate.started_at();
    let completed_at = aggregate.completed_at();
    let mut response_state = aggregate.response_state();
    if aggregate.needs_user_input(now_seconds()) {
        response_state = Some("needsInput".to_string());
    }
    let was_interrupted = aggregate.was_interrupted();
    let has_completed_turn = aggregate.has_completed_turn();
    Some(AIRuntimeContextSnapshot {
        tool: "claude".to_string(),
        external_session_id: Some(external_id),
        transcript_path,
        model: aggregate.model,
        assistant_preview: aggregate.assistant_preview,
        input_tokens: aggregate.input_tokens,
        output_tokens: aggregate.output_tokens,
        cached_input_tokens: aggregate.cached_input_tokens,
        total_tokens: aggregate.total_tokens,
        usage_amounts: Vec::new(),
        baseline_usage_amounts: Vec::new(),
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
    /// Most recent assistant message carrying a `tool_use` block, and the most
    /// recent user message carrying its `tool_result`. While a tool is blocked on
    /// a permission prompt the `tool_use` is written but no `tool_result`
    /// follows, so `last_tool_use_at > last_tool_result_at` is the pending-call
    /// signature behind `needsInput` detection.
    last_tool_use_at: f64,
    last_tool_result_at: f64,
    /// Last `permission-mode` entry's mode (e.g. `bypassPermissions`, `default`,
    /// `acceptEdits`, `plan`). `bypassPermissions` means no prompt can ever fire,
    /// so a pending call there is the CLI working, not waiting.
    permission_mode: Option<String>,
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
            last_tool_use_at: self.last_tool_use_at.max(other.last_tool_use_at),
            last_tool_result_at: self.last_tool_result_at.max(other.last_tool_result_at),
            permission_mode: other.permission_mode.or(self.permission_mode),
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

    /// Whether the session's permission mode can still raise an approval prompt.
    /// Only `bypassPermissions` (codux's `--dangerously-skip-permissions`) silences
    /// every prompt; `default`/`acceptEdits`/`plan` all still gate some action.
    /// Unknown/absent (older CLIs) defaults to `true` so a wait is never silently
    /// dropped.
    fn prompts_possible(&self) -> bool {
        !matches!(self.permission_mode.as_deref(), Some("bypassPermissions"))
    }

    /// A `tool_use` is written with no matching `tool_result` yet -- the call is
    /// in flight. Combined with an idle gap (in the caller) this is the
    /// permission/elicitation wait signature.
    fn pending_tool_call(&self) -> bool {
        self.last_tool_use_at > 0.0 && self.last_tool_use_at > self.last_tool_result_at
    }

    /// The session is mid-turn with a tool call that has been written but left
    /// unanswered past the idle gap, in a mode that can still prompt -- i.e. the
    /// CLI is blocked waiting on the user. Idle is measured from the tool-use
    /// row's own timestamp, not `updated_at`, because timestamp-less metadata
    /// rows (permission-mode/mode/ai-title) pin `updated_at` to `now` on every
    /// read.
    fn needs_user_input(&self, now: f64) -> bool {
        is_awaiting_user_decision(
            self.response_state().as_deref() == Some("responding"),
            self.prompts_possible(),
            self.pending_tool_call(),
            self.last_tool_use_at,
            now,
        )
    }
}

fn claude_message_has_block(message: &Value, block_type: &str) -> bool {
    message
        .get("content")
        .and_then(|content| content.as_array())
        .map(|items| {
            items
                .iter()
                .any(|item| item.get("type").and_then(|value| value.as_str()) == Some(block_type))
        })
        .unwrap_or(false)
}

fn parse_claude_log_runtime_state(
    file_path: &Path,
    project_path: &str,
    external_session_id: &str,
) -> Option<ClaudeAggregate> {
    let file = fs::File::open(file_path).ok()?;
    let mut reader = BufReader::new(file);
    let mut aggregate = ClaudeAggregate::default();
    let mut matched = false;
    let mut line = String::new();

    loop {
        line.clear();
        let Ok(bytes) = reader.read_line(&mut line) else {
            break;
        };
        if bytes == 0 {
            break;
        }
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
        if is_claude_control_command_row(&row) {
            continue;
        }
        let timestamp = row
            .get("timestamp")
            .and_then(|value| value.as_str())
            .and_then(parse_iso8601_seconds)
            .unwrap_or_else(now_seconds);
        aggregate.updated_at = aggregate.updated_at.max(timestamp);
        // The current permission mode rides its own row (no message/role, and --
        // unlike message rows -- no timestamp). Capture it for the prompt-wait
        // gate; last one in file order wins.
        if row.get("type").and_then(|value| value.as_str()) == Some("permission-mode") {
            if let Some(mode) = row
                .get("permissionMode")
                .and_then(|value| value.as_str())
                .and_then(|value| normalized_string(Some(value)))
            {
                aggregate.permission_mode = Some(mode);
            }
            continue;
        }
        let message = row.get("message").unwrap_or(&Value::Null);
        let role = message
            .get("role")
            .and_then(|value| value.as_str())
            .or_else(|| row.get("type").and_then(|value| value.as_str()));
        if role == Some("user") {
            // A `tool_result` answers a pending `tool_use`; record it so the
            // pending-call signature clears once the result lands. Tool results
            // are never interruptions, so track them regardless of the branch.
            if claude_message_has_block(message, "tool_result") {
                aggregate.last_tool_result_at = aggregate.last_tool_result_at.max(timestamp);
            }
            if is_claude_interrupted_row(&row) {
                aggregate.last_interrupted_at = aggregate.last_interrupted_at.max(timestamp);
                aggregate.last_completion_at = aggregate.last_completion_at.max(timestamp);
            } else {
                aggregate.last_user_at = aggregate.last_user_at.max(timestamp);
            }
        } else if role == Some("assistant") {
            if claude_message_has_block(message, "tool_use") {
                aggregate.last_tool_use_at = aggregate.last_tool_use_at.max(timestamp);
            }
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

fn parse_claude_task_tool_uses(message: &Value, timestamp: f64, aggregate: &mut ClaudeAggregate) {
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

/// A genuine user interruption is recorded by Claude as a `user` row whose
/// message text is exactly its marker -- `[Request interrupted by user]` or
/// `[Request interrupted by user for tool use]`. Match ONLY that marker.
///
/// The previous heuristic stringified the whole row and scanned for
/// "interrupted"/"cancelled"/"aborted" anywhere. Those words are everyday in
/// command/tool output (e.g. "operation cancelled", "connection aborted", "no
/// matches"), so tool-result `user` rows were constantly misread as turn
/// interruptions. That pushed `last_completion_at` past `last_user_at`, flipping
/// `response_state` to idle and demoting a live turn to "completed" -- the
/// session showed no running state even while Claude was clearly working.
fn is_claude_interrupted_row(row: &Value) -> bool {
    claude_user_message_text(row)
        .map(|text| {
            text.trim_start()
                .starts_with("[Request interrupted by user")
        })
        .unwrap_or(false)
}

fn is_claude_control_command_row(row: &Value) -> bool {
    if row.get("type").and_then(Value::as_str) != Some("user") {
        return false;
    }
    claude_user_message_text(row)
        .map(|text| {
            let text = text.trim_start();
            text.starts_with("<local-command-") || text.starts_with("<command-name>")
        })
        .unwrap_or(false)
}

/// The user message's plain text (string content, or the concatenated `text`
/// blocks of array content). Tool results carry `tool_result` blocks rather than
/// `text`, so they never contribute here -- exactly what keeps their incidental
/// wording from being mistaken for the interrupt marker.
fn claude_user_message_text(row: &Value) -> Option<String> {
    match row.get("message")?.get("content")? {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => {
            let mut out = String::new();
            for item in items {
                if item.get("type").and_then(Value::as_str) == Some("text") {
                    if let Some(text) = item.get("text").and_then(Value::as_str) {
                        out.push_str(text);
                    }
                }
            }
            (!out.is_empty()).then_some(out)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::constants::NEEDS_INPUT_IDLE_SECONDS;

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

    #[test]
    fn tool_result_wording_is_not_an_interrupt() {
        // Everyday command/tool output mentioning these words must NOT register
        // as a turn interruption.
        let block = serde_json::json!({
            "type": "user",
            "message": {"role": "user", "content": [
                {"type": "tool_result", "content": "error: operation cancelled\nbuild aborted"}
            ]}
        });
        let text = serde_json::json!({
            "type": "user",
            "message": {"role": "user", "content": "why was the deploy aborted?"}
        });
        assert!(!is_claude_interrupted_row(&block));
        assert!(!is_claude_interrupted_row(&text));
    }

    #[test]
    fn genuine_interrupt_marker_is_detected() {
        let string_form = serde_json::json!({
            "type": "user",
            "message": {"role": "user", "content": "[Request interrupted by user]"}
        });
        let block_form = serde_json::json!({
            "type": "user",
            "message": {"role": "user", "content": [
                {"type": "text", "text": "[Request interrupted by user for tool use]"}
            ]}
        });
        assert!(is_claude_interrupted_row(&string_form));
        assert!(is_claude_interrupted_row(&block_form));
    }

    #[test]
    fn live_turn_with_cancel_wording_stays_responding() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("codux-claude-probe-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.jsonl");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(
            file,
            r#"{{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:00Z","type":"user","message":{{"role":"user","content":"do the thing"}}}}"#
        )
        .unwrap();
        // Tool result whose output mentions "cancelled" -- mid-turn activity,
        // not an interruption. The turn must still read as responding.
        writeln!(
            file,
            r#"{{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:05Z","type":"user","message":{{"role":"user","content":[{{"type":"tool_result","content":"error: operation cancelled"}}]}}}}"#
        )
        .unwrap();
        drop(file);

        let aggregate = parse_claude_log_runtime_state(&path, "/tmp/p", "s1").expect("aggregate");
        assert_eq!(aggregate.response_state().as_deref(), Some("responding"));
        assert!(!aggregate.was_interrupted());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn local_slash_commands_do_not_start_a_runtime_turn() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("codux-claude-local-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.jsonl");
        let mut file = std::fs::File::create(&path).unwrap();
        writeln!(
            file,
            r#"{{"type":"permission-mode","permissionMode":"bypassPermissions","sessionId":"s1"}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:00Z","type":"user","message":{{"role":"user","content":"<local-command-caveat>Caveat: local command</local-command-caveat>"}}}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:00Z","type":"user","message":{{"role":"user","content":"<command-name>/effort</command-name>\n<command-message>effort</command-message>"}}}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:00Z","type":"user","message":{{"role":"user","content":"<local-command-stdout>Set effort level to xhigh</local-command-stdout>"}}}}"#
        )
        .unwrap();
        drop(file);

        let aggregate = parse_claude_log_runtime_state(&path, "/tmp/p", "s1").expect("aggregate");

        assert_eq!(aggregate.response_state(), None);
        assert_eq!(aggregate.last_user_at, 0.0);
        assert_eq!(aggregate.last_completion_at, 0.0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pending_tool_use_past_idle_gap_is_a_user_wait() {
        let aggregate = ClaudeAggregate {
            last_user_at: 10.0,
            last_tool_use_at: 12.0,
            ..Default::default()
        };
        assert!(aggregate.pending_tool_call());
        assert!(aggregate.prompts_possible());
        assert_eq!(aggregate.response_state().as_deref(), Some("responding"));
        // Still fresh -> could be a fast auto-approved call, not a wait yet.
        assert!(!aggregate.needs_user_input(12.0 + NEEDS_INPUT_IDLE_SECONDS - 0.5));
        // Idle past the gap -> blocked on the user.
        assert!(aggregate.needs_user_input(12.0 + NEEDS_INPUT_IDLE_SECONDS + 0.5));
    }

    #[test]
    fn bypass_permissions_never_waits() {
        let aggregate = ClaudeAggregate {
            last_user_at: 10.0,
            last_tool_use_at: 12.0,
            permission_mode: Some("bypassPermissions".to_string()),
            ..Default::default()
        };
        assert!(!aggregate.prompts_possible());
        assert!(!aggregate.needs_user_input(1_000.0));
    }

    #[test]
    fn answered_tool_call_is_not_a_wait() {
        let aggregate = ClaudeAggregate {
            last_user_at: 10.0,
            last_tool_use_at: 12.0,
            last_tool_result_at: 13.0,
            ..Default::default()
        };
        assert!(!aggregate.pending_tool_call());
        assert!(!aggregate.needs_user_input(1_000.0));
    }

    #[test]
    fn parses_permission_mode_and_pending_tool_use_from_transcript() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("codux-claude-wait-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.jsonl");
        let mut file = std::fs::File::create(&path).unwrap();
        // permission-mode rides its own (timestamp-less) row.
        writeln!(
            file,
            r#"{{"type":"permission-mode","permissionMode":"default","sessionId":"s1"}}"#
        )
        .unwrap();
        writeln!(
            file,
            r#"{{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:00Z","type":"user","message":{{"role":"user","content":"run the build"}}}}"#
        )
        .unwrap();
        // Assistant emits a tool_use (stop_reason tool_use), no tool_result follows.
        writeln!(
            file,
            r#"{{"sessionId":"s1","cwd":"/tmp/p","timestamp":"2026-01-01T00:00:02Z","type":"assistant","message":{{"role":"assistant","stop_reason":"tool_use","content":[{{"type":"tool_use","id":"t1","name":"Bash","input":{{"command":"make"}}}}]}}}}"#
        )
        .unwrap();
        drop(file);

        let aggregate = parse_claude_log_runtime_state(&path, "/tmp/p", "s1").expect("aggregate");
        assert_eq!(aggregate.permission_mode.as_deref(), Some("default"));
        assert!(aggregate.pending_tool_call());
        assert!(aggregate.last_tool_use_at > 0.0);
        assert_eq!(aggregate.response_state().as_deref(), Some("responding"));
        assert!(
            aggregate.needs_user_input(aggregate.last_tool_use_at + NEEDS_INPUT_IDLE_SECONDS + 1.0)
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
