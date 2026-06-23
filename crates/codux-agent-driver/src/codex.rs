//! Codex driver: builds the `codex app-server` invocation, performs the
//! initialize/thread-start handshake, and translates the app-server's
//! ServerNotification / ServerRequest stream into the normalized [`AgentEvent`]
//! model. This is the per-CLI piece; the JSON-RPC transport and timeline merge
//! are shared.

use std::process::Command;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::thread;

use parking_lot::Mutex;
use serde_json::{Value, json};

use crate::event::{AgentEvent, ApprovalDecision, ApprovalRequest, TokenUsage};
use crate::jsonrpc::{Inbound, JsonRpcClient};
use crate::timeline::{ItemStatus, Timeline, TimelineItem, TimelineKind};
use crate::{AgentDriver, AgentInvocation, AgentTransport, SessionConfig};

/// Registered driver. `program` is the codex executable — ideally the *real*
/// codex binary (not the codux wrapper), spawned directly over pipes. `env`
/// supplies extra environment (notably `PATH`, so codex's `env node` shebang and
/// its own subprocesses resolve). Spawning the real binary directly avoids both
/// interactive-shell init and the codux wrapper's PATH re-resolution (which can
/// ping-pong between two codux installs' wrappers).
pub struct CodexAgentDriver {
    pub program: String,
    pub env: Vec<(String, String)>,
}

impl Default for CodexAgentDriver {
    fn default() -> Self {
        Self {
            program: "codex".to_string(),
            env: Vec::new(),
        }
    }
}

impl AgentDriver for CodexAgentDriver {
    fn id(&self) -> &str {
        "codex"
    }

    fn transport(&self) -> AgentTransport {
        AgentTransport::CodexAppServer
    }

    fn invocation(&self, _cfg: &SessionConfig) -> AgentInvocation {
        // Keep the child non-interactive; it must not try to drive a TTY.
        let mut env = vec![
            ("TERM".into(), "dumb".into()),
            ("TERM_PROGRAM".into(), "codux-agent".into()),
        ];
        env.extend(self.env.iter().cloned());
        AgentInvocation {
            program: self.program.clone(),
            args: vec!["app-server".into(), "--listen".into(), "stdio://".into()],
            env,
        }
    }
}

type Sink = Box<dyn Fn(&AgentEvent) + Send + Sync>;

/// A selectable model from the server's `model/list` (not hardcoded). Each model
/// advertises its own supported reasoning efforts and a default.
#[derive(Clone, Debug)]
pub struct CodexModel {
    pub id: String,
    pub display_name: String,
    pub is_default: bool,
    pub supported_efforts: Vec<String>,
    pub default_effort: String,
}

/// A user/project skill from `skills/list` (scoped to the session cwd).
#[derive(Clone, Debug)]
pub struct CodexSkill {
    pub name: String,
    pub description: String,
}

/// A permission/sandbox profile from `permissionProfile/list` (e.g. `:read-only`,
/// `:workspace`, `:danger-full-access`).
#[derive(Clone, Debug)]
pub struct CodexPermissionProfile {
    pub id: String,
    pub description: Option<String>,
}

struct Pending {
    id: Value,
    method: String,
}

/// A live codex conversation. Cheap to clone (`Arc` inside).
#[derive(Clone)]
pub struct CodexSession {
    inner: Arc<Inner>,
}

struct Inner {
    client: Arc<JsonRpcClient>,
    timeline: Mutex<Timeline>,
    thread_id: String,
    current_turn: Mutex<Option<String>>,
    pending_approvals: Mutex<std::collections::HashMap<String, Pending>>,
    /// Model override applied to subsequent turns (set via `/model`).
    model: Mutex<Option<String>>,
    /// Reasoning effort applied to subsequent turns (set via `/effort`).
    effort: Mutex<Option<String>>,
    sink: Sink,
}

impl CodexSession {
    /// Spawn the app-server, complete the handshake, and start translating the
    /// event stream into `sink`. Blocks only for the synchronous handshake.
    pub fn start(
        driver: &CodexAgentDriver,
        cfg: &SessionConfig,
        sink: Sink,
    ) -> Result<Self, String> {
        let inv = driver.invocation(cfg);
        let mut cmd = Command::new(&inv.program);
        cmd.args(&inv.args);
        for (k, v) in &inv.env {
            cmd.env(k, v);
        }
        cmd.current_dir(&cfg.cwd);

        let (client, inbound) = JsonRpcClient::spawn(cmd).map_err(|e| e.to_string())?;

        // 1) initialize
        client.request(
            "initialize",
            json!({ "clientInfo": { "name": "codux", "version": env!("CARGO_PKG_VERSION") } }),
        )?;
        // 2) initialized (notification, no params)
        client.notify("initialized", Value::Null)?;
        // 3) thread/start -> thread id
        let mut start = json!({
            "cwd": cfg.cwd,
            "approvalPolicy": cfg.approval_policy,
            "sandbox": cfg.sandbox,
        });
        if let Some(model) = &cfg.model {
            start["model"] = json!(model);
        }
        let res = client.request("thread/start", start)?;
        let thread_id = res
            .get("thread")
            .and_then(|t| t.get("id"))
            .and_then(Value::as_str)
            .ok_or("thread/start returned no thread.id")?
            .to_string();

        let inner = Arc::new(Inner {
            client,
            timeline: Mutex::new(Timeline::default()),
            thread_id: thread_id.clone(),
            current_turn: Mutex::new(None),
            pending_approvals: Mutex::new(std::collections::HashMap::new()),
            model: Mutex::new(cfg.model.clone()),
            effort: Mutex::new(None),
            sink,
        });

        (inner.sink)(&AgentEvent::ThreadStarted {
            thread_id: thread_id.clone(),
        });

        // Consume the event stream on a background thread.
        {
            let inner = inner.clone();
            thread::spawn(move || consume(inner, inbound));
        }

        Ok(Self { inner })
    }

    pub fn thread_id(&self) -> &str {
        &self.inner.thread_id
    }

    /// Send a user turn. Returns once the server acks; completion arrives as
    /// events (TurnCompleted).
    pub fn send_user_message(&self, text: &str) -> Result<(), String> {
        let mut params = json!({
            "threadId": self.inner.thread_id,
            "input": [{ "type": "text", "text": text }],
        });
        if let Some(model) = self.inner.model.lock().clone() {
            params["model"] = json!(model);
        }
        if let Some(effort) = self.inner.effort.lock().clone() {
            params["effort"] = json!(effort);
        }
        self.inner.client.request("turn/start", params)?;
        Ok(())
    }

    /// Set the model used for subsequent turns (the `/model` command).
    pub fn set_model(&self, model: Option<String>) {
        *self.inner.model.lock() = model;
    }

    /// Set the reasoning effort for subsequent turns (the `/effort` command).
    pub fn set_effort(&self, effort: Option<String>) {
        *self.inner.effort.lock() = effort;
    }

    /// Compact the thread's context (the `/compact` command).
    pub fn compact(&self) -> Result<(), String> {
        self.inner.client.request(
            "thread/compact/start",
            json!({ "threadId": self.inner.thread_id }),
        )?;
        Ok(())
    }

    /// Answer an [`ApprovalRequest`] previously emitted to the sink.
    pub fn respond_approval(&self, token: &str, decision: ApprovalDecision) -> Result<(), String> {
        let pending = self
            .inner
            .pending_approvals
            .lock()
            .remove(token)
            .ok_or("unknown approval token")?;
        self.inner
            .client
            .respond(pending.id, json!({ "decision": decision.wire() }))
            .map(|_| {
                let _ = pending.method; // method kept for future per-kind responses
            })
    }

    pub fn interrupt(&self) -> Result<(), String> {
        let turn = self.inner.current_turn.lock().clone();
        let Some(turn_id) = turn else {
            return Ok(());
        };
        self.inner.client.request(
            "turn/interrupt",
            json!({ "threadId": self.inner.thread_id, "turnId": turn_id }),
        )?;
        Ok(())
    }

    /// Fetch the server's model catalog (`model/list`). Blocking; call off-thread.
    pub fn list_models(&self) -> Result<Vec<CodexModel>, String> {
        let res = self.inner.client.request("model/list", json!({}))?;
        let data = res
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(data
            .iter()
            .filter(|m| !m.get("hidden").and_then(Value::as_bool).unwrap_or(false))
            .map(|m| {
                let id = m
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let display_name = m
                    .get("displayName")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty())
                    .unwrap_or(&id)
                    .to_string();
                let supported_efforts = m
                    .get("supportedReasoningEfforts")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|e| {
                                e.get("reasoningEffort").and_then(Value::as_str).map(String::from)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                CodexModel {
                    is_default: m.get("isDefault").and_then(Value::as_bool).unwrap_or(false),
                    default_effort: m
                        .get("defaultReasoningEffort")
                        .and_then(Value::as_str)
                        .unwrap_or("medium")
                        .to_string(),
                    supported_efforts,
                    display_name,
                    id,
                }
            })
            .collect())
    }

    /// Fetch the user/project skills visible for `cwd` (`skills/list`). Blocking.
    pub fn list_skills(&self, cwd: &str) -> Result<Vec<CodexSkill>, String> {
        let res = self
            .inner
            .client
            .request("skills/list", json!({ "cwds": [cwd] }))?;
        let mut out = Vec::new();
        if let Some(groups) = res.get("data").and_then(Value::as_array) {
            for group in groups {
                if let Some(skills) = group.get("skills").and_then(Value::as_array) {
                    for s in skills {
                        if !s.get("enabled").and_then(Value::as_bool).unwrap_or(true) {
                            continue;
                        }
                        if let Some(name) = s.get("name").and_then(Value::as_str) {
                            out.push(CodexSkill {
                                name: name.to_string(),
                                description: s
                                    .get("description")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string(),
                            });
                        }
                    }
                }
            }
        }
        Ok(out)
    }

    /// Fetch the available permission/sandbox profiles (`permissionProfile/list`).
    pub fn list_permission_profiles(
        &self,
        cwd: &str,
    ) -> Result<Vec<CodexPermissionProfile>, String> {
        let res = self
            .inner
            .client
            .request("permissionProfile/list", json!({ "cwd": cwd }))?;
        let data = res
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(data
            .iter()
            .filter_map(|p| {
                let id = p.get("id").and_then(Value::as_str)?.to_string();
                Some(CodexPermissionProfile {
                    id,
                    description: p
                        .get("description")
                        .and_then(Value::as_str)
                        .map(String::from),
                })
            })
            .collect())
    }

    /// Snapshot of the merged timeline (clones the items).
    pub fn timeline_snapshot(&self) -> Vec<TimelineItem> {
        self.inner.timeline.lock().items().to_vec()
    }

    pub fn shutdown(&self) {
        self.inner.client.kill();
    }
}

/// Translate inbound frames into events, applying merges to the timeline first.
fn consume(inner: Arc<Inner>, inbound: Receiver<Inbound>) {
    for frame in inbound {
        match frame {
            Inbound::Notification { method, params } => {
                for ev in notification_to_events(&inner, &method, &params) {
                    apply(&inner, &ev);
                    (inner.sink)(&ev);
                }
            }
            Inbound::ServerRequest { id, method, params } => {
                let ev = server_request_to_event(&inner, id, &method, &params);
                apply(&inner, &ev);
                (inner.sink)(&ev);
            }
        }
    }
}

fn apply(inner: &Inner, ev: &AgentEvent) {
    let mut tl = inner.timeline.lock();
    match ev {
        AgentEvent::ItemStarted(it) | AgentEvent::ItemCompleted(it) => tl.upsert(it.clone()),
        AgentEvent::MessageDelta { id, text } => {
            tl.append_text(id, text, TimelineKind::AssistantMessage, "agentMessage")
        }
        AgentEvent::ReasoningDelta { id, text } => {
            tl.append_text(id, text, TimelineKind::Reasoning, "reasoning")
        }
        AgentEvent::CommandOutputDelta { id, text } => tl.append_output(id, text),
        _ => {}
    }
}

fn notification_to_events(inner: &Inner, method: &str, params: &Value) -> Vec<AgentEvent> {
    match method {
        "turn/started" => {
            if let Some(id) = params
                .get("turn")
                .and_then(|t| t.get("id"))
                .and_then(Value::as_str)
            {
                *inner.current_turn.lock() = Some(id.to_string());
            }
            vec![AgentEvent::TurnStarted]
        }
        "turn/completed" => {
            *inner.current_turn.lock() = None;
            vec![AgentEvent::TurnCompleted]
        }
        "item/started" => params
            .get("item")
            .map(|it| vec![AgentEvent::ItemStarted(build_item(it, false))])
            .unwrap_or_default(),
        "item/completed" => params
            .get("item")
            .map(|it| vec![AgentEvent::ItemCompleted(build_item(it, true))])
            .unwrap_or_default(),
        "item/agentMessage/delta" => delta_event(params, |id, d| AgentEvent::MessageDelta {
            id,
            text: d,
        }),
        "item/reasoning/textDelta" => delta_event(params, |id, d| AgentEvent::ReasoningDelta {
            id,
            text: d,
        }),
        "item/commandExecution/outputDelta" => {
            delta_event(params, |id, d| AgentEvent::CommandOutputDelta { id, text: d })
        }
        "thread/tokenUsage/updated" => params
            .get("tokenUsage")
            .map(|u| vec![AgentEvent::TokenUsage(parse_usage(u))])
            .unwrap_or_default(),
        "error" => vec![AgentEvent::Error(
            params
                .get("error")
                .map(ToString::to_string)
                .unwrap_or_else(|| params.to_string()),
        )],
        // Lifecycle/status chatter we keep as a low-priority status signal.
        "thread/status/changed" | "thread/compacted" | "account/rateLimits/updated"
        | "turn/diff/updated" | "turn/plan/updated" => vec![AgentEvent::Status(method.to_string())],
        _ => Vec::new(),
    }
}

fn delta_event(params: &Value, make: impl Fn(String, String) -> AgentEvent) -> Vec<AgentEvent> {
    let id = params.get("itemId").and_then(Value::as_str);
    let delta = params.get("delta").and_then(Value::as_str);
    match (id, delta) {
        (Some(id), Some(d)) => vec![make(id.to_string(), d.to_string())],
        _ => Vec::new(),
    }
}

fn server_request_to_event(inner: &Inner, id: Value, method: &str, params: &Value) -> AgentEvent {
    let token = match &id {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let summary = match method {
        "item/commandExecution/requestApproval" => params
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or("(command)")
            .to_string(),
        "item/fileChange/requestApproval" => params
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("apply file changes")
            .to_string(),
        _ => method.to_string(),
    };
    inner.pending_approvals.lock().insert(
        token.clone(),
        Pending {
            id,
            method: method.to_string(),
        },
    );
    AgentEvent::ApprovalRequest(ApprovalRequest {
        token,
        method: method.to_string(),
        summary,
        raw: params.clone(),
    })
}

fn parse_usage(u: &Value) -> TokenUsage {
    let total = u.get("total").cloned().unwrap_or(Value::Null);
    let g = |k: &str| total.get(k).and_then(Value::as_u64).unwrap_or(0);
    TokenUsage {
        total_tokens: g("totalTokens"),
        input_tokens: g("inputTokens"),
        output_tokens: g("outputTokens"),
        cached_input_tokens: g("cachedInputTokens"),
        reasoning_output_tokens: g("reasoningOutputTokens"),
        model_context_window: u.get("modelContextWindow").and_then(Value::as_u64),
    }
}

fn kind_for(item_type: &str) -> TimelineKind {
    match item_type {
        "userMessage" | "hookPrompt" => TimelineKind::UserPrompt,
        "agentMessage" => TimelineKind::AssistantMessage,
        "reasoning" => TimelineKind::Reasoning,
        "plan" => TimelineKind::Plan,
        "commandExecution" => TimelineKind::Command,
        "fileChange" => TimelineKind::FileChange,
        // mcpToolCall / dynamicToolCall / webSearch / imageGeneration / … all
        // render as a generic tool card; the raw item carries the specifics.
        _ => TimelineKind::ToolCall,
    }
}

/// Pull the human-visible text out of any item type without per-type special
/// casing: prefer `text`, else concat `content[].text`, else `summary[].text`.
fn item_text(item: &Value) -> String {
    if let Some(t) = item.get("text").and_then(Value::as_str) {
        return t.to_string();
    }
    let mut out = String::new();
    for key in ["content", "summary"] {
        if let Some(arr) = item.get(key).and_then(Value::as_array) {
            for el in arr {
                if let Some(t) = el.get("text").and_then(Value::as_str) {
                    out.push_str(t);
                }
            }
            if !out.is_empty() {
                break;
            }
        }
    }
    out
}

fn parse_status(item: &Value) -> ItemStatus {
    match item.get("status").and_then(Value::as_str) {
        Some(s) if s.contains("fail") || s.contains("error") => ItemStatus::Failed,
        Some(s) if s.contains("complet") || s.contains("success") || s == "done" => {
            ItemStatus::Completed
        }
        _ => ItemStatus::InProgress,
    }
}

fn build_item(item: &Value, completed: bool) -> TimelineItem {
    let item_type = item
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let kind = kind_for(&item_type);
    // Many item types (userMessage, reasoning, agentMessage) carry no `status`
    // field, so an `item/completed` for them must be marked done explicitly.
    let status = match (parse_status(item), completed) {
        (ItemStatus::InProgress, true) => ItemStatus::Completed,
        (s, _) => s,
    };
    let command = item.get("command").and_then(Value::as_str).map(String::from);
    let title = match kind {
        TimelineKind::Command => command.clone().unwrap_or_else(|| item_type.clone()),
        TimelineKind::ToolCall => item
            .get("tool")
            .and_then(Value::as_str)
            .unwrap_or(&item_type)
            .to_string(),
        _ => item_type.clone(),
    };
    TimelineItem {
        id: item
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        kind,
        item_type,
        title,
        text: item_text(item),
        command,
        cwd: item.get("cwd").and_then(Value::as_str).map(String::from),
        exit_code: item.get("exitCode").and_then(Value::as_i64),
        output: item
            .get("aggregatedOutput")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        status,
        raw: item.clone(),
    }
}
