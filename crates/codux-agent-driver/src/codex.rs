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

use crate::event::{
    AgentEvent, ApprovalDecision, ApprovalRequest, PermissionRequest, PlanStep, TokenUsage,
    UserInputOption, UserInputQuestion, UserInputRequest,
};
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
pub struct AgentModel {
    pub id: String,
    pub display_name: String,
    pub is_default: bool,
    pub supported_efforts: Vec<String>,
    pub default_effort: String,
}

/// A user/project skill from `skills/list` (scoped to the session cwd).
#[derive(Clone, Debug)]
pub struct AgentSkill {
    pub name: String,
    pub description: String,
    pub path: String,
}

/// A permission/sandbox profile from `permissionProfile/list` (e.g. `:read-only`,
/// `:workspace`, `:danger-full-access`).
#[derive(Clone, Debug)]
pub struct AgentPermissionProfile {
    pub id: String,
    pub description: Option<String>,
}

/// One part of a user turn. Mirrors codex's `UserInput` union: plain text, an
/// explicit skill invocation, an @-file mention, or an attached local image.
#[derive(Clone, Debug)]
pub enum UserInputPart {
    Text(String),
    Skill { name: String, path: String },
    Mention { name: String, path: String },
    LocalImage { path: String },
}

/// A fuzzy file-search hit (`fuzzyFileSearch`), used to back the @-mention picker.
#[derive(Clone, Debug)]
pub struct FileHit {
    pub root: String,
    pub path: String,
    pub file_name: String,
    pub score: i64,
}

/// A prior conversation thread for this cwd (`thread/list`), for the resume picker.
#[derive(Clone, Debug)]
pub struct ThreadInfo {
    pub id: String,
    pub preview: String,
    pub updated_at: i64,
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

    /// Resume a prior thread by id (`thread/resume`): hydrate the timeline from
    /// the returned history turns, then stream live events as usual.
    pub fn resume(
        driver: &CodexAgentDriver,
        cfg: &SessionConfig,
        thread_id: &str,
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
        client.request(
            "initialize",
            json!({ "clientInfo": { "name": "codux", "version": env!("CARGO_PKG_VERSION") } }),
        )?;
        client.notify("initialized", Value::Null)?;
        let mut params = json!({
            "threadId": thread_id,
            "cwd": cfg.cwd,
            "approvalPolicy": cfg.approval_policy,
            "sandbox": cfg.sandbox,
        });
        if let Some(model) = &cfg.model {
            params["model"] = json!(model);
        }
        let res = client.request("thread/resume", params)?;
        let thread = res.get("thread");
        let tid = thread
            .and_then(|t| t.get("id"))
            .and_then(Value::as_str)
            .unwrap_or(thread_id)
            .to_string();

        // Hydrate the merged timeline from the thread's history turns.
        let mut timeline = Timeline::default();
        if let Some(turns) = thread.and_then(|t| t.get("turns")).and_then(Value::as_array) {
            for turn in turns {
                if let Some(items) = turn.get("items").and_then(Value::as_array) {
                    for it in items {
                        timeline.upsert(build_item(it, true));
                    }
                }
            }
        }

        let inner = Arc::new(Inner {
            client,
            timeline: Mutex::new(timeline),
            thread_id: tid.clone(),
            current_turn: Mutex::new(None),
            pending_approvals: Mutex::new(std::collections::HashMap::new()),
            model: Mutex::new(cfg.model.clone()),
            effort: Mutex::new(None),
            sink,
        });
        (inner.sink)(&AgentEvent::ThreadStarted {
            thread_id: tid.clone(),
        });
        {
            let inner = inner.clone();
            thread::spawn(move || consume(inner, inbound));
        }
        Ok(Self { inner })
    }

    pub fn thread_id(&self) -> &str {
        &self.inner.thread_id
    }

    /// Send a plain-text user turn (convenience over [`Self::send_user_turn`]).
    pub fn send_user_message(&self, text: &str) -> Result<(), String> {
        self.send_user_turn(vec![UserInputPart::Text(text.to_string())])
    }

    /// Send a user turn made of mixed input parts (text + skills + mentions +
    /// images). Returns once the server acks; completion arrives as events.
    pub fn send_user_turn(&self, parts: Vec<UserInputPart>) -> Result<(), String> {
        let input: Vec<Value> = parts
            .iter()
            .map(|p| match p {
                UserInputPart::Text(text) => json!({ "type": "text", "text": text }),
                UserInputPart::Skill { name, path } => {
                    json!({ "type": "skill", "name": name, "path": path })
                }
                UserInputPart::Mention { name, path } => {
                    json!({ "type": "mention", "name": name, "path": path })
                }
                UserInputPart::LocalImage { path } => {
                    json!({ "type": "localImage", "path": path })
                }
            })
            .collect();
        let mut params = json!({
            "threadId": self.inner.thread_id,
            "input": input,
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

    /// Fuzzy file search for the @-mention picker (`fuzzyFileSearch`). Blocking.
    pub fn search_files(&self, query: &str, roots: Vec<String>) -> Result<Vec<FileHit>, String> {
        let res = self.inner.client.request(
            "fuzzyFileSearch",
            json!({ "query": query, "roots": roots, "cancellationToken": null }),
        )?;
        Ok(res
            .get("files")
            .and_then(Value::as_array)
            .map(|files| {
                files
                    .iter()
                    .filter_map(|f| {
                        Some(FileHit {
                            root: f.get("root").and_then(Value::as_str)?.to_string(),
                            path: f.get("path").and_then(Value::as_str)?.to_string(),
                            file_name: f
                                .get("file_name")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                            score: f.get("score").and_then(Value::as_i64).unwrap_or(0),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// List prior threads recorded for `cwd` (`thread/list`), newest first.
    pub fn list_threads(&self, cwd: &str) -> Result<Vec<ThreadInfo>, String> {
        let res = self
            .inner
            .client
            .request("thread/list", json!({ "cwd": cwd, "limit": 40 }))?;
        Ok(res
            .get("data")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| {
                        let id = t.get("id").and_then(Value::as_str)?.to_string();
                        Some(ThreadInfo {
                            id,
                            preview: t
                                .get("preview")
                                .and_then(Value::as_str)
                                .or_else(|| t.get("name").and_then(Value::as_str))
                                .unwrap_or("(无标题)")
                                .to_string(),
                            updated_at: t
                                .get("updatedAt")
                                .or_else(|| t.get("recencyAt"))
                                .and_then(Value::as_i64)
                                .unwrap_or(0),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Rewind the thread by dropping the last `num_turns` turns
    /// (`thread/rollback`) — the server side of editing a past message.
    /// Does NOT revert file changes the agent already made.
    pub fn rollback(&self, num_turns: u32) -> Result<(), String> {
        if num_turns == 0 {
            return Ok(());
        }
        self.inner.client.request(
            "thread/rollback",
            json!({ "threadId": self.inner.thread_id, "numTurns": num_turns }),
        )?;
        Ok(())
    }

    /// How many user turns exist at-or-after the given timeline item id (i.e. the
    /// number of turns a `thread/rollback` must drop to rewind to before it).
    pub fn turns_from(&self, item_id: &str) -> u32 {
        let tl = self.inner.timeline.lock();
        let mut seen_target = false;
        let mut count = 0u32;
        for it in tl.items() {
            if it.id == item_id {
                seen_target = true;
            }
            if seen_target && it.kind == TimelineKind::UserPrompt {
                count += 1;
            }
        }
        count
    }

    /// Mirror a rollback in the local timeline: drop the edited item and after.
    pub fn truncate_timeline_before(&self, item_id: &str) {
        self.inner.timeline.lock().truncate_before(item_id);
    }

    /// Start a code review of the working tree's uncommitted changes
    /// (`review/start`, target = uncommittedChanges).
    pub fn review_uncommitted(&self) -> Result<(), String> {
        self.inner.client.request(
            "review/start",
            json!({
                "threadId": self.inner.thread_id,
                "target": { "type": "uncommittedChanges" },
            }),
        )?;
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
        let pending = self.take_pending(token)?;
        // MCP elicitations answer with `{action}`, everything else `{decision}`.
        let result = if pending.method == "mcpServer/elicitation/request" {
            let action = match decision {
                ApprovalDecision::Accept | ApprovalDecision::AcceptForSession => "accept",
                ApprovalDecision::Decline => "decline",
                ApprovalDecision::Cancel => "cancel",
            };
            json!({ "action": action })
        } else {
            json!({ "decision": decision.wire() })
        };
        self.inner.client.respond(pending.id, result)
    }

    /// Approve a command AND persist the proposed execpolicy amendment so
    /// similar commands stop prompting.
    pub fn respond_approval_amendment(
        &self,
        token: &str,
        amendment: Vec<String>,
    ) -> Result<(), String> {
        let pending = self.take_pending(token)?;
        self.inner.client.respond(
            pending.id,
            json!({ "decision": { "acceptWithExecpolicyAmendment": {
                "execpolicy_amendment": amendment,
            } } }),
        )
    }

    /// Answer an [`crate::event::UserInputRequest`]: `(question id, answers)`.
    pub fn respond_user_input(
        &self,
        token: &str,
        answers: Vec<(String, Vec<String>)>,
    ) -> Result<(), String> {
        let pending = self.take_pending(token)?;
        let map: serde_json::Map<String, Value> = answers
            .into_iter()
            .map(|(qid, ans)| (qid, json!({ "answers": ans })))
            .collect();
        self.inner
            .client
            .respond(pending.id, json!({ "answers": map }))
    }

    /// Answer a [`crate::event::PermissionRequest`]: grant `granted` (echo the
    /// requested profile, or `{}` to deny) for `scope` (`turn` | `session`).
    pub fn respond_permissions(
        &self,
        token: &str,
        granted: Value,
        scope: &str,
    ) -> Result<(), String> {
        let pending = self.take_pending(token)?;
        self.inner
            .client
            .respond(pending.id, json!({ "permissions": granted, "scope": scope }))
    }

    fn take_pending(&self, token: &str) -> Result<Pending, String> {
        self.inner
            .pending_approvals
            .lock()
            .remove(token)
            .ok_or_else(|| "unknown approval token".to_string())
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
    pub fn list_models(&self) -> Result<Vec<AgentModel>, String> {
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
                AgentModel {
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
    pub fn list_skills(&self, cwd: &str) -> Result<Vec<AgentSkill>, String> {
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
                            out.push(AgentSkill {
                                name: name.to_string(),
                                description: s
                                    .get("description")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string(),
                                path: s
                                    .get("path")
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
    ) -> Result<Vec<AgentPermissionProfile>, String> {
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
                Some(AgentPermissionProfile {
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
        AgentEvent::PlanDelta { id, text } => {
            tl.append_text(id, text, TimelineKind::Plan, "plan")
        }
        AgentEvent::FileChangesUpdated { id, changes } => tl.set_changes(id, changes.clone()),
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
        // GPT-5-class models stream reasoning as summaries, not raw CoT — both
        // delta channels feed the same reasoning card.
        "item/reasoning/textDelta" | "item/reasoning/summaryTextDelta" => {
            delta_event(params, |id, d| AgentEvent::ReasoningDelta { id, text: d })
        }
        "item/commandExecution/outputDelta" | "item/fileChange/outputDelta" => {
            delta_event(params, |id, d| AgentEvent::CommandOutputDelta { id, text: d })
        }
        "item/plan/delta" => delta_event(params, |id, d| AgentEvent::PlanDelta { id, text: d }),
        "item/fileChange/patchUpdated" => params
            .get("itemId")
            .and_then(Value::as_str)
            .zip(params.get("changes"))
            .map(|(id, changes)| {
                vec![AgentEvent::FileChangesUpdated {
                    id: id.to_string(),
                    changes: changes.clone(),
                }]
            })
            .unwrap_or_default(),
        "turn/plan/updated" => {
            let steps = params
                .get("plan")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|s| {
                            Some(PlanStep {
                                step: s.get("step").and_then(Value::as_str)?.to_string(),
                                status: s
                                    .get("status")
                                    .and_then(Value::as_str)
                                    .unwrap_or("pending")
                                    .to_string(),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            vec![AgentEvent::TurnPlan {
                explanation: params
                    .get("explanation")
                    .and_then(Value::as_str)
                    .map(String::from),
                steps,
            }]
        }
        "thread/name/updated" => params
            .get("threadName")
            .and_then(Value::as_str)
            .filter(|name| !name.trim().is_empty())
            .map(|name| vec![AgentEvent::ThreadNameUpdated(name.to_string())])
            .unwrap_or_default(),
        "thread/compacted" | "model/rerouted" | "warning" | "configWarning"
        | "deprecationNotice" | "guardianWarning" => vec![AgentEvent::Notice {
            kind: method.to_string(),
            message: params
                .get("message")
                .or_else(|| params.get("text"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        }],
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
        "thread/status/changed" | "account/rateLimits/updated" | "turn/diff/updated" => {
            vec![AgentEvent::Status(method.to_string())]
        }
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
    inner.pending_approvals.lock().insert(
        token.clone(),
        Pending {
            id,
            method: method.to_string(),
        },
    );
    match method {
        // Mid-turn questions: answered with `{answers}` — not a decision.
        "item/tool/requestUserInput" => {
            let questions = params
                .get("questions")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(|q| {
                            Some(UserInputQuestion {
                                id: q.get("id").and_then(Value::as_str)?.to_string(),
                                header: q
                                    .get("header")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string(),
                                question: q
                                    .get("question")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string(),
                                options: q
                                    .get("options")
                                    .and_then(Value::as_array)
                                    .map(|opts| {
                                        opts.iter()
                                            .filter_map(|o| {
                                                Some(UserInputOption {
                                                    label: o
                                                        .get("label")
                                                        .and_then(Value::as_str)?
                                                        .to_string(),
                                                    description: o
                                                        .get("description")
                                                        .and_then(Value::as_str)
                                                        .unwrap_or_default()
                                                        .to_string(),
                                                })
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                                is_other: q.get("isOther").and_then(Value::as_bool).unwrap_or(false),
                                is_secret: q
                                    .get("isSecret")
                                    .and_then(Value::as_bool)
                                    .unwrap_or(false),
                            })
                        })
                        .collect()
                })
                .unwrap_or_default();
            AgentEvent::UserInputRequest(UserInputRequest { token, questions })
        }
        // Permission escalation: answered with `{permissions, scope}`.
        "item/permissions/requestApproval" => AgentEvent::PermissionRequest(PermissionRequest {
            token,
            reason: params.get("reason").and_then(Value::as_str).map(String::from),
            cwd: params
                .get("cwd")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            requested: params.get("permissions").cloned().unwrap_or(Value::Null),
        }),
        _ => {
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
                "mcpServer/elicitation/request" => params
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or(method)
                    .to_string(),
                _ => method.to_string(),
            };
            AgentEvent::ApprovalRequest(ApprovalRequest {
                token,
                method: method.to_string(),
                summary,
                raw: params.clone(),
            })
        }
    }
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

impl crate::session::AgentSession for CodexSession {
    fn capabilities(&self) -> crate::session::SessionCapabilities {
        // codex app-server supports the full feature set.
        crate::session::SessionCapabilities {
            models: true,
            efforts: true,
            skills: true,
            permission_profiles: true,
            file_search: true,
            rollback: true,
            review: true,
            compact: true,
            history: true,
        }
    }
    fn send_user_turn(&self, parts: Vec<UserInputPart>) -> Result<(), String> {
        CodexSession::send_user_turn(self, parts)
    }
    fn set_model(&self, model: Option<String>) {
        CodexSession::set_model(self, model)
    }
    fn set_effort(&self, effort: Option<String>) {
        CodexSession::set_effort(self, effort)
    }
    fn compact(&self) -> Result<(), String> {
        CodexSession::compact(self)
    }
    fn interrupt(&self) -> Result<(), String> {
        CodexSession::interrupt(self)
    }
    fn rollback(&self, num_turns: u32) -> Result<(), String> {
        CodexSession::rollback(self, num_turns)
    }
    fn turns_from(&self, item_id: &str) -> u32 {
        CodexSession::turns_from(self, item_id)
    }
    fn truncate_timeline_before(&self, item_id: &str) {
        CodexSession::truncate_timeline_before(self, item_id)
    }
    fn review_uncommitted(&self) -> Result<(), String> {
        CodexSession::review_uncommitted(self)
    }
    fn respond_approval(&self, token: &str, decision: ApprovalDecision) -> Result<(), String> {
        CodexSession::respond_approval(self, token, decision)
    }
    fn respond_approval_amendment(&self, token: &str, amendment: Vec<String>) -> Result<(), String> {
        CodexSession::respond_approval_amendment(self, token, amendment)
    }
    fn respond_user_input(&self, token: &str, answers: Vec<(String, Vec<String>)>) -> Result<(), String> {
        CodexSession::respond_user_input(self, token, answers)
    }
    fn respond_permissions(&self, token: &str, granted: Value, scope: &str) -> Result<(), String> {
        CodexSession::respond_permissions(self, token, granted, scope)
    }
    fn list_models(&self) -> Result<Vec<AgentModel>, String> {
        CodexSession::list_models(self)
    }
    fn list_skills(&self, cwd: &str) -> Result<Vec<AgentSkill>, String> {
        CodexSession::list_skills(self, cwd)
    }
    fn list_permission_profiles(&self, cwd: &str) -> Result<Vec<AgentPermissionProfile>, String> {
        CodexSession::list_permission_profiles(self, cwd)
    }
    fn search_files(&self, query: &str, roots: Vec<String>) -> Result<Vec<FileHit>, String> {
        CodexSession::search_files(self, query, roots)
    }
    fn list_threads(&self, cwd: &str) -> Result<Vec<crate::codex::ThreadInfo>, String> {
        CodexSession::list_threads(self, cwd)
    }
    fn timeline_snapshot(&self) -> Vec<TimelineItem> {
        CodexSession::timeline_snapshot(self)
    }
    fn shutdown(&self) {
        CodexSession::shutdown(self)
    }
}
