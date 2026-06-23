//! GPUI chat view for a protocol-driven AI session (Codex).
//!
//! Renders the agent-driver's merged timeline as left/right message bubbles with
//! per-message actions, a Codex-style composer (access mode, model, effort,
//! round send button) and a `/` command menu. The session runs off-thread and is
//! bound to the active worktree; events arrive over a flume channel drained in a
//! `cx.spawn` loop so the UI thread never blocks.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use std::sync::Arc;

use chrono::Local;
use codux_agent_driver::{
    AgentEvent, AgentKind, AgentModel, AgentPermissionProfile, AgentSession, AgentSkill,
    ApprovalDecision, ApprovalRequest, FileHit, ItemStatus, SessionConfig, TimelineItem,
    TimelineKind, TokenUsage, UserInputPart, start_session,
};
use flume::Sender;
use gpui::{
    AnyElement, AppContext, ClipboardItem, Context, Div, Entity, EventEmitter, InteractiveElement,
    IntoElement, ParentElement, PathPromptOptions, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement, Styled, Task, WeakEntity, Window, div, img,
    prelude::FluentBuilder as _, px, rems,
};
use gpui_component::{
    ActiveTheme, Icon, Sizable, Size,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    menu::{DropdownMenu, PopupMenuItem},
    text::TextView,
};

use crate::app::types::{WorkspaceSplitKind, WorkspaceView};
use crate::heroicons::HeroIconName;

impl crate::app::CoduxApp {
    /// Toggle the body-split AI chat panel (Codex on the right of the terminal).
    pub(in crate::app) fn toggle_chat_split(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.workspace_split == Some(WorkspaceSplitKind::Chat) {
            self.workspace_split = None;
        } else {
            self.workspace_view = WorkspaceView::Terminal;
            self.workspace_split = Some(WorkspaceSplitKind::Chat);
        }
        cx.notify();
    }
}

/// Sandbox / approval posture, shown in the composer.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Access {
    ReadOnly,
    WorkspaceWrite,
    FullAccess,
}

impl Access {
    fn label(self) -> &'static str {
        match self {
            Access::ReadOnly => "只读",
            Access::WorkspaceWrite => "工作区写入",
            Access::FullAccess => "完全访问",
        }
    }
    fn sandbox(self) -> &'static str {
        match self {
            Access::ReadOnly => "read-only",
            Access::WorkspaceWrite => "workspace-write",
            Access::FullAccess => "danger-full-access",
        }
    }
    fn approval(self) -> &'static str {
        match self {
            Access::FullAccess => "never",
            _ => "on-request",
        }
    }
    /// Map a server permission-profile id (e.g. `:workspace`) to our posture.
    fn from_profile_id(id: &str) -> Access {
        match id {
            s if s.contains("read-only") => Access::ReadOnly,
            s if s.contains("full-access") || s.contains("danger") => Access::FullAccess,
            _ => Access::WorkspaceWrite,
        }
    }
}

/// One slash command (also shown in the `+` menu and the `/` palette).
struct Command {
    icon: HeroIconName,
    name: &'static str,
    desc: &'static str,
    /// What to insert/run. A trailing space means "fill the box" (takes an arg).
    token: &'static str,
}

fn commands() -> Vec<Command> {
    // Action verbs only — model/effort/access live in the composer dropdowns, and
    // skills are listed dynamically below these.
    vec![
        Command { icon: HeroIconName::PlusCircle, name: "/new", desc: "新建一个对话标签", token: "/new" },
        Command { icon: HeroIconName::ShieldCheck, name: "/review", desc: "审查工作区未提交的改动", token: "/review" },
        Command { icon: HeroIconName::ArchiveBoxArrowDown, name: "/compact", desc: "压缩此对话的上下文", token: "/compact" },
        Command { icon: HeroIconName::StopCircle, name: "/interrupt", desc: "中断当前回合", token: "/interrupt" },
        Command { icon: HeroIconName::ChartBar, name: "/status", desc: "查看本会话 token 用量", token: "/status" },
    ]
}

/// An attachment chip shown above the composer; sent as a structured input part.
#[derive(Clone)]
enum Attachment {
    Skill { name: String, path: String },
    Mention { name: String, path: String },
    Image { path: String },
}

impl Attachment {
    fn label(&self) -> String {
        match self {
            Attachment::Skill { name, .. } => format!("技能 {name}"),
            Attachment::Mention { name, .. } => format!("@{name}"),
            Attachment::Image { path } => std::path::Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "图片".into()),
        }
    }
    fn icon(&self) -> HeroIconName {
        match self {
            Attachment::Skill { .. } => HeroIconName::Sparkles,
            Attachment::Mention { .. } => HeroIconName::Document,
            Attachment::Image { .. } => HeroIconName::Photo,
        }
    }
    fn to_part(&self) -> UserInputPart {
        match self {
            Attachment::Skill { name, path } => UserInputPart::Skill {
                name: name.clone(),
                path: path.clone(),
            },
            Attachment::Mention { name, path } => UserInputPart::Mention {
                name: name.clone(),
                path: path.clone(),
            },
            Attachment::Image { path } => UserInputPart::LocalImage { path: path.clone() },
        }
    }
}

/// Event from a chat tab up to its panel (for tab-level actions like /new).
pub(in crate::app) enum ChatViewEvent {
    NewTab,
}

/// Messages from the off-thread session machinery into the view.
enum ChatMsg {
    Note(String),
    Started(Arc<dyn AgentSession>),
    /// The server's dynamic catalog (models / skills / permission profiles),
    /// fetched after the session starts — these drive the composer dropdowns and
    /// the `/` palette instead of a hardcoded list.
    Catalog {
        models: Vec<AgentModel>,
        skills: Vec<AgentSkill>,
        profiles: Vec<AgentPermissionProfile>,
    },
    /// Results of an @-mention fuzzy file search (query echoed to ignore stale).
    FileHits { query: String, hits: Vec<FileHit> },
    Failed(String),
    Event(AgentEvent),
}

/// Resolve the real codex binary + a working PATH via a one-shot interactive-login
/// shell (so nvm/.zshrc PATH is in effect), skipping any codux wrapper dir, so we
/// can spawn codex directly (the wrapper adds nothing for app-server and its PATH
/// re-resolution can ping-pong between two codux installs).
fn resolve_codex(wrapper_fallback: &str) -> (String, String) {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let script = r##"setopt sh_word_split 2>/dev/null; IFS=:; for d in $PATH; do case "$d" in *wrappers/bin) continue;; esac; if [ -x "$d/codex" ]; then printf 'CODUX_BIN=%s\n' "$d/codex"; break; fi; done; printf 'CODUX_PATH=%s\n' "$PATH""##;
    let mut program = wrapper_fallback.to_string();
    let mut path_env = String::new();
    if let Ok(out) = std::process::Command::new(&shell)
        .args(["-lic", script])
        .output()
    {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            if let Some(p) = line.strip_prefix("CODUX_BIN=") {
                if !p.is_empty() {
                    program = p.to_string();
                }
            } else if let Some(p) = line.strip_prefix("CODUX_PATH=") {
                path_env = p.to_string();
            }
        }
    }
    (program, path_env)
}

/// Spawn the codex session off-thread: resolve the binary, handshake, fetch the
/// dynamic catalog (models/skills/profiles), and report each step back over `tx`.
/// No first prompt is sent — the session waits idle for user input, so the
/// composer can populate before anything is typed.
fn spawn_session(
    tx: Sender<ChatMsg>,
    cwd: String,
    wrapper: String,
    access: Access,
    model: Option<String>,
    effort: String,
) {
    std::thread::spawn(move || {
        let _ = tx.send(ChatMsg::Note("解析 codex…".into()));
        let (program, path_env) = resolve_codex(&wrapper);
        let _ = tx.send(ChatMsg::Note("启动 app-server…".into()));
        let mut env = Vec::new();
        if !path_env.is_empty() {
            env.push(("PATH".to_string(), path_env));
        }
        let cfg = SessionConfig {
            cwd,
            model,
            approval_policy: access.approval().to_string(),
            sandbox: access.sandbox().to_string(),
        };
        let sink_tx = tx.clone();
        let sink = Box::new(move |ev: &AgentEvent| {
            let _ = sink_tx.send(ChatMsg::Event(ev.clone()));
        });
        let _ = tx.send(ChatMsg::Note("握手中…".into()));
        // The driver kind is the single switch point for codex / claude / opencode.
        match start_session(AgentKind::Codex, program, env, &cfg, sink) {
            Ok(session) => {
                session.set_effort(Some(effort));
                let _ = tx.send(ChatMsg::Started(session.clone()));
                // Pull the dynamic catalog so the composer shows real
                // models/efforts/skills, not a hardcoded guess.
                let models = session.list_models().unwrap_or_default();
                let skills = session.list_skills(&cfg.cwd).unwrap_or_default();
                let profiles = session.list_permission_profiles(&cfg.cwd).unwrap_or_default();
                let _ = tx.send(ChatMsg::Catalog {
                    models,
                    skills,
                    profiles,
                });
            }
            Err(err) => {
                let _ = tx.send(ChatMsg::Failed(err));
            }
        }
    });
}

pub(in crate::app) struct ChatView {
    cwd: String,
    codex_program: String,
    session: Option<Arc<dyn AgentSession>>,
    starting: bool,
    /// A turn is in flight (drives the send/stop button state).
    busy: bool,
    /// When the current turn started (for the "Working (Ns)" elapsed counter).
    turn_started: Option<Instant>,
    /// Turns composed before the session finished connecting; flushed on Started.
    pending_send: Vec<Vec<UserInputPart>>,
    items: Vec<TimelineItem>,
    pending_approvals: Vec<ApprovalRequest>,
    item_times: HashMap<String, SharedString>,
    status: SharedString,
    input: Entity<InputState>,
    scroll: ScrollHandle,
    access: Access,
    model: Option<String>,
    effort: SharedString,
    /// Server catalog (empty until the session reports it).
    models: Vec<AgentModel>,
    skills: Vec<AgentSkill>,
    profiles: Vec<AgentPermissionProfile>,
    /// Structured input parts staged for the next turn (skills, @-files, images).
    attachments: Vec<Attachment>,
    /// Last token usage seen, surfaced by /status.
    last_usage: Option<TokenUsage>,
    /// Active @-mention query and its current hits (drives the file picker).
    mention_query: Option<String>,
    mention_hits: Vec<FileHit>,
    /// Inline edit in progress: (user item id, its edit input).
    editing: Option<(String, Entity<InputState>)>,
    /// Activity cards (commands/reasoning/tools) the user has expanded.
    expanded: std::collections::HashSet<String>,
    /// Explicit open/closed state for consecutive-activity groups (keyed by the
    /// group's first item id); absent = default (open while running, else closed).
    group_open: std::collections::HashMap<String, bool>,
    tx: Sender<ChatMsg>,
    _drain: Task<()>,
    /// 1s heartbeat that repaints the elapsed "Working (Ns)" counter while busy.
    _tick: Task<()>,
}

impl EventEmitter<ChatViewEvent> for ChatView {}

impl ChatView {
    pub(in crate::app) fn new(
        cwd: String,
        codex_program: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (tx, rx) = flume::unbounded::<ChatMsg>();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("给 Codex 发消息…  (输入 / 调出命令，Shift+Enter 换行)")
                .multi_line(true)
                .submit_on_enter(true)
        });
        cx.subscribe_in(&input, window, |view, _input, event, window, cx| match event {
            InputEvent::PressEnter { shift, .. } if !*shift => view.submit(window, cx),
            InputEvent::Change => view.on_input_changed(cx),
            _ => {}
        })
        .detach();

        let drain = cx.spawn(async move |this: WeakEntity<Self>, cx| {
            while let Ok(first) = rx.recv_async().await {
                // Coalesce everything already queued into one render pass.
                let mut batch = vec![first];
                while let Ok(more) = rx.try_recv() {
                    batch.push(more);
                }
                if this
                    .update(cx, |view, cx| view.handle_batch(batch, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        // 1s heartbeat: repaint the elapsed "Working (Ns)" counter while a turn runs.
        let timer = cx.background_executor().clone();
        let tick = cx.spawn(async move |this: WeakEntity<Self>, cx| {
            loop {
                timer.timer(Duration::from_secs(1)).await;
                let alive = this
                    .update(cx, |view, cx| {
                        if view.busy {
                            cx.notify();
                        }
                    })
                    .is_ok();
                if !alive {
                    break;
                }
            }
        });
        // Connect eagerly so the model/effort/access dropdowns and the `/` palette
        // populate from the server before the user types anything.
        spawn_session(
            tx.clone(),
            cwd.clone(),
            codex_program.clone(),
            Access::WorkspaceWrite,
            None,
            "medium".to_string(),
        );
        Self {
            cwd,
            codex_program,
            session: None,
            starting: true,
            busy: false,
            turn_started: None,
            pending_send: Vec::new(),
            items: Vec::new(),
            pending_approvals: Vec::new(),
            item_times: HashMap::new(),
            status: SharedString::from("空闲"),
            input,
            scroll: ScrollHandle::new(),
            access: Access::WorkspaceWrite,
            model: None,
            effort: SharedString::from("medium"),
            models: Vec::new(),
            skills: Vec::new(),
            profiles: Vec::new(),
            attachments: Vec::new(),
            last_usage: None,
            mention_query: None,
            mention_hits: Vec::new(),
            editing: None,
            expanded: std::collections::HashSet::new(),
            group_open: std::collections::HashMap::new(),
            tx,
            _drain: drain,
            _tick: tick,
        }
    }

    /// Short label for the tab bar: a snippet of the first user message.
    pub(in crate::app) fn title(&self) -> SharedString {
        self.items
            .iter()
            .find(|i| i.kind == TimelineKind::UserPrompt && !i.text.trim().is_empty())
            .map(|i| {
                let line = i.text.trim().lines().next().unwrap_or_default();
                let snippet: String = line.chars().take(16).collect();
                if line.chars().count() > 16 {
                    SharedString::from(format!("{snippet}…"))
                } else {
                    SharedString::from(snippet)
                }
            })
            .unwrap_or_else(|| SharedString::from("新对话"))
    }

    /// Apply a burst of messages with a SINGLE snapshot + render. Streaming emits
    /// many delta events per second; cloning the timeline and re-rendering per
    /// event is what made it lag, so we coalesce a whole batch into one update.
    fn handle_batch(&mut self, batch: Vec<ChatMsg>, cx: &mut Context<Self>) {
        // Decide follow BEFORE adding content (reflects the pre-batch scroll pos).
        let follow = self.near_bottom();
        let mut refresh = false;
        for msg in batch {
            refresh |= self.apply_msg(msg);
        }
        if refresh {
            if let Some(session) = &self.session {
                self.items = session.timeline_snapshot();
            }
            let now = Local::now().format("%H:%M").to_string();
            for item in &self.items {
                self.item_times
                    .entry(item.id.clone())
                    .or_insert_with(|| SharedString::from(now.clone()));
            }
        }
        if follow {
            self.scroll.scroll_to_bottom();
        }
        cx.notify();
    }

    /// Apply one message to state (no snapshot / notify). Returns true if the
    /// timeline may have changed (so the batch refreshes items once at the end).
    fn apply_msg(&mut self, msg: ChatMsg) -> bool {
        match msg {
            ChatMsg::Note(note) => {
                self.status = SharedString::from(note);
                false
            }
            ChatMsg::Started(session) => {
                self.session = Some(session);
                self.starting = false;
                self.status = SharedString::from("就绪");
                self.pump_queue();
                self.update_queue_status();
                true
            }
            ChatMsg::Catalog {
                models,
                skills,
                profiles,
            } => {
                if self.model.is_none()
                    && let Some(def) = models.iter().find(|m| m.is_default)
                {
                    self.model = Some(def.id.clone());
                    self.effort = SharedString::from(def.default_effort.clone());
                    if let Some(session) = &self.session {
                        session.set_effort(Some(def.default_effort.clone()));
                    }
                }
                self.models = models;
                self.skills = skills;
                self.profiles = profiles;
                false
            }
            ChatMsg::FileHits { query, hits } => {
                if self.mention_query.as_deref() == Some(query.as_str()) {
                    self.mention_hits = hits;
                }
                false
            }
            ChatMsg::Failed(err) => {
                self.starting = false;
                self.busy = false;
                self.turn_started = None;
                self.status = SharedString::from(format!("错误: {err}"));
                false
            }
            ChatMsg::Event(ev) => {
                match ev {
                    AgentEvent::ApprovalRequest(req) => self.pending_approvals.push(req),
                    AgentEvent::TurnCompleted => {
                        self.busy = false;
                        self.turn_started = None;
                        self.status = SharedString::from("就绪");
                        self.pump_queue();
                        self.update_queue_status();
                    }
                    AgentEvent::TokenUsage(u) => self.last_usage = Some(u),
                    AgentEvent::Error(err) => {
                        self.busy = false;
                        self.turn_started = None;
                        self.status = SharedString::from(format!("错误: {err}"));
                        self.pump_queue();
                        self.update_queue_status();
                    }
                    AgentEvent::Status(_) => {}
                    _ => self.status = SharedString::from("生成中…"),
                }
                true
            }
        }
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().trim().to_string();

        // A bare slash line (no attachments) is a command, not a message.
        if self.attachments.is_empty() && text.starts_with('/') {
            self.input
                .update(cx, |state, cx| state.set_value("", window, cx));
            self.run_command(&text, window, cx);
            return;
        }

        // Build the turn from the typed text plus any staged attachments.
        let mut parts: Vec<UserInputPart> = Vec::new();
        if !text.is_empty() {
            parts.push(UserInputPart::Text(text));
        }
        parts.extend(self.attachments.iter().map(Attachment::to_part));
        if parts.is_empty() {
            return;
        }

        self.input
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.attachments.clear();
        self.mention_query = None;
        self.mention_hits.clear();

        // Always queue, then pump: a turn sends immediately when idle, otherwise
        // it waits its turn (no double-send / interrupt while generating).
        self.pending_send.push(parts);
        if self.session.is_none() {
            self.ensure_session();
        }
        self.pump_queue();
        self.update_queue_status();
        self.scroll.scroll_to_bottom(); // user's own send always jumps to bottom
        cx.notify();
    }

    /// True if the message list is scrolled to within ~120px of the bottom, i.e.
    /// the user is "following" and we may auto-scroll on new content.
    fn near_bottom(&self) -> bool {
        let off = self.scroll.offset().y; // <= 0 as content scrolls up
        let max = self.scroll.max_offset().y; // >= 0 total scrollable
        (max + off) <= px(120.0)
    }

    /// Send the next queued turn if the session is idle (not busy) and connected.
    fn pump_queue(&mut self) {
        if self.busy || self.pending_send.is_empty() {
            return;
        }
        let Some(session) = self.session.clone() else {
            return;
        };
        let parts = self.pending_send.remove(0);
        self.busy = true;
        self.turn_started = Some(Instant::now());
        self.status = SharedString::from("生成中…");
        std::thread::spawn(move || {
            let _ = session.send_user_turn(parts);
        });
    }

    /// Reflect queue/connection state in the status line.
    fn update_queue_status(&mut self) {
        if !self.pending_send.is_empty() {
            self.status = SharedString::from(format!("已排队 {} 条", self.pending_send.len()));
        } else if self.session.is_none() {
            self.status = SharedString::from("连接中…");
        }
    }

    /// Interrupt the in-flight turn (the send button's stop state).
    fn stop(&mut self, cx: &mut Context<Self>) {
        if let Some(session) = self.session.clone() {
            std::thread::spawn(move || {
                let _ = session.interrupt();
            });
        }
        self.pending_send.clear();
        self.busy = false;
        self.turn_started = None;
        self.status = SharedString::from("已中断");
        cx.notify();
    }

    /// Turn a past user message into an inline editor (codex-style). Enter
    /// rewinds the thread to before it and resends; Esc/取消 cancels.
    fn begin_edit(&mut self, id: String, text: String, window: &mut Window, cx: &mut Context<Self>) {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .submit_on_enter(true)
        });
        input.update(cx, |s, cx| s.set_value(&text, window, cx));
        cx.subscribe_in(&input, window, |view, _i, ev, window, cx| {
            if let InputEvent::PressEnter { shift, .. } = ev
                && !*shift
            {
                view.commit_edit(window, cx);
            }
        })
        .detach();
        input.update(cx, |s, cx| s.focus(window, cx));
        self.editing = Some((id, input));
        cx.notify();
    }

    fn cancel_edit(&mut self, cx: &mut Context<Self>) {
        self.editing = None;
        cx.notify();
    }

    fn toggle_expand(&mut self, id: String, cx: &mut Context<Self>) {
        if !self.expanded.remove(&id) {
            self.expanded.insert(id);
        }
        cx.notify();
    }

    fn toggle_group(&mut self, key: String, currently_open: bool, cx: &mut Context<Self>) {
        self.group_open.insert(key, !currently_open);
        cx.notify();
    }

    /// Commit an inline edit: rollback to before the message, then resend it —
    /// which drops every later message, exactly like codex.
    fn commit_edit(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, input)) = self.editing.take() else {
            return;
        };
        let text = input.read(cx).value().trim().to_string();
        let Some(session) = self.session.clone() else {
            cx.notify();
            return;
        };
        if text.is_empty() {
            cx.notify();
            return;
        }
        let num_turns = session.turns_from(&id);
        // Truncate the local view immediately for snappy feedback.
        if let Some(pos) = self.items.iter().position(|it| it.id == id) {
            self.items.truncate(pos);
        }
        self.busy = true;
        self.status = SharedString::from("重发中…");
        std::thread::spawn(move || {
            let _ = session.rollback(num_turns);
            session.truncate_timeline_before(&id);
            let _ = session.send_user_message(&text);
        });
        cx.notify();
    }

    /// Start the codex session if it isn't already up. Called eagerly when the
    /// view is created so the composer's model/effort/access dropdowns and the `/`
    /// palette's skills populate from the server *before* the first message.
    fn ensure_session(&mut self) {
        if self.session.is_some() || self.starting {
            return;
        }
        self.starting = true;
        self.status = SharedString::from("连接中…");
        spawn_session(
            self.tx.clone(),
            self.cwd.clone(),
            self.codex_program.clone(),
            self.access,
            self.model.clone(),
            self.effort.to_string(),
        );
    }

    fn run_command(&mut self, line: &str, window: &mut Window, cx: &mut Context<Self>) {
        let line = line.trim();
        let mut parts = line.splitn(2, char::is_whitespace);
        let cmd = parts.next().unwrap_or("");
        let arg = parts.next().unwrap_or("").trim();

        // Commands that don't require a live session.
        match cmd {
            "/new" => {
                cx.emit(ChatViewEvent::NewTab);
                return;
            }
            "/status" => {
                self.status = self.usage_summary();
                cx.notify();
                return;
            }
            _ => {}
        }

        let Some(session) = self.session.clone() else {
            self.status = SharedString::from("正在连接 Codex…");
            self.ensure_session();
            cx.notify();
            return;
        };

        match cmd {
            "/interrupt" => {
                std::thread::spawn(move || {
                    let _ = session.interrupt();
                });
                self.status = SharedString::from("中断中…");
            }
            "/compact" => {
                std::thread::spawn(move || {
                    let _ = session.compact();
                });
                self.status = SharedString::from("压缩中…");
            }
            "/review" => {
                std::thread::spawn(move || {
                    let _ = session.review_uncommitted();
                });
                self.status = SharedString::from("审查未提交改动中…");
            }
            "/model" => {
                if arg.is_empty() {
                    self.set_input("/model ", window, cx);
                    self.status = SharedString::from("用法: /model <名称>");
                } else {
                    session.set_model(Some(arg.to_string()));
                    self.model = Some(arg.to_string());
                    self.status = SharedString::from(format!("模型: {arg}"));
                }
            }
            "/effort" => {
                if arg.is_empty() {
                    self.set_input("/effort ", window, cx);
                    self.status = SharedString::from("用法: /effort <low|medium|high|xhigh>");
                } else {
                    session.set_effort(Some(arg.to_string()));
                    self.effort = SharedString::from(arg.to_string());
                    self.status = SharedString::from(format!("推理强度: {arg}"));
                }
            }
            other => self.status = SharedString::from(format!("未知命令: {other}")),
        }
        cx.notify();
    }

    fn set_input(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |state, cx| {
            state.set_value(text, window, cx);
            state.focus(window, cx);
        });
    }

    /// The indicator stays up for the entire turn (until completion), never
    /// disappearing mid-turn.
    fn show_working(&self) -> bool {
        self.busy
    }

    /// State-dependent verb for the working indicator.
    fn working_word(&self) -> &'static str {
        match self.items.last() {
            Some(it) => match it.kind {
                TimelineKind::Reasoning => "思考中",
                TimelineKind::Command => "执行命令",
                TimelineKind::FileChange => "修改文件",
                TimelineKind::ToolCall => "调用工具",
                TimelineKind::AssistantMessage if it.status == ItemStatus::InProgress => "撰写回复",
                _ => "工作中",
            },
            None => "工作中",
        }
    }

    fn usage_summary(&self) -> SharedString {
        match &self.last_usage {
            Some(u) => SharedString::from(format!(
                "用量 · 合计 {} (输入 {} / 输出 {} / 缓存 {})",
                u.total_tokens, u.input_tokens, u.output_tokens, u.cached_input_tokens
            )),
            None => SharedString::from("暂无 token 用量数据"),
        }
    }

    /// React to input edits: extract the active `@mention` token (a trailing
    /// `@word` with no whitespace) and kick a fuzzy file search for it.
    fn on_input_changed(&mut self, cx: &mut Context<Self>) {
        let value = self.input.read(cx).value().to_string();
        let query = active_mention(&value);
        if query == self.mention_query {
            return;
        }
        self.mention_query = query.clone();
        if let Some(q) = query {
            if let Some(session) = self.session.clone() {
                let tx = self.tx.clone();
                let root = self.cwd.clone();
                let q2 = q.clone();
                std::thread::spawn(move || {
                    let hits = session.search_files(&q2, vec![root]).unwrap_or_default();
                    let _ = tx.send(ChatMsg::FileHits { query: q2, hits });
                });
            }
        } else {
            self.mention_hits.clear();
        }
        cx.notify();
    }

    /// Add a skill attachment (from the `/` palette) to the next turn.
    fn add_skill(&mut self, name: String, path: String, window: &mut Window, cx: &mut Context<Self>) {
        self.attachments.push(Attachment::Skill { name, path });
        // Clear the leading `/` filter from the box.
        self.input
            .update(cx, |state, cx| state.set_value("", window, cx));
        cx.notify();
    }

    /// Resolve a chosen @-file into a Mention attachment and strip the `@token`.
    fn add_mention(&mut self, hit: FileHit, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.input.read(cx).value().to_string();
        let stripped = strip_active_mention(&value);
        self.input
            .update(cx, |state, cx| state.set_value(&stripped, window, cx));
        self.attachments.push(Attachment::Mention {
            name: hit.file_name.clone(),
            path: hit.path.clone(),
        });
        self.mention_query = None;
        self.mention_hits.clear();
        cx.notify();
    }

    fn remove_attachment(&mut self, idx: usize, cx: &mut Context<Self>) {
        if idx < self.attachments.len() {
            self.attachments.remove(idx);
            cx.notify();
        }
    }

    /// Open a native picker to attach one or more local images.
    fn pick_images(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some(SharedString::from("选择图片")),
        });
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                let _ = this.update(cx, |view, cx| {
                    for p in paths {
                        view.attachments.push(Attachment::Image {
                            path: p.to_string_lossy().to_string(),
                        });
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// A command chosen from the `/` palette or `+` menu.
    fn command_clicked(&mut self, token: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        if token.ends_with(' ') {
            self.set_input(token, window, cx);
        } else {
            self.input
                .update(cx, |state, cx| state.set_value("", window, cx));
            self.run_command(token, window, cx);
        }
    }

    fn respond_approval(
        &mut self,
        token: String,
        decision: ApprovalDecision,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = &self.session {
            let _ = session.respond_approval(&token, decision);
        }
        self.pending_approvals.retain(|req| req.token != token);
        cx.notify();
    }

    fn set_access(&mut self, access: Access, cx: &mut Context<Self>) {
        self.access = access;
        if self.session.is_some() {
            self.status = SharedString::from(format!("{} (重启会话后生效)", access.label()));
        }
        cx.notify();
    }

    fn set_model_value(&mut self, model: Option<String>, cx: &mut Context<Self>) {
        if let Some(session) = &self.session {
            session.set_model(model.clone());
        }
        // Switching model retargets effort to that model's default if the current
        // effort isn't one it supports.
        if let Some(id) = &model
            && let Some(m) = self.models.iter().find(|m| &m.id == id)
            && !m.supported_efforts.iter().any(|e| e == self.effort.as_ref())
        {
            self.effort = SharedString::from(m.default_effort.clone());
            if let Some(session) = &self.session {
                session.set_effort(Some(m.default_effort.clone()));
            }
        }
        self.model = model;
        cx.notify();
    }

    fn set_effort_value(&mut self, effort: String, cx: &mut Context<Self>) {
        if let Some(session) = &self.session {
            session.set_effort(Some(effort.clone()));
        }
        self.effort = SharedString::from(effort);
        cx.notify();
    }
}

#[derive(Clone, Copy)]
struct Pal {
    fg: gpui::Hsla,
    muted: gpui::Hsla,
    border: gpui::Hsla,
    secondary: gpui::Hsla,
    primary: gpui::Hsla,
    primary_fg: gpui::Hsla,
    danger: gpui::Hsla,
    bubble: gpui::Hsla,
}

impl Render for ChatView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let pal = Pal {
            fg: theme.foreground,
            muted: theme.muted_foreground,
            border: theme.border,
            secondary: theme.secondary,
            primary: theme.primary,
            primary_fg: theme.primary_foreground,
            danger: theme.danger,
            bubble: theme.secondary,
        };
        let input_bg = theme.background;
        let mono = theme.mono_font_family.clone();

        let input_value = self.input.read(cx).value().to_string();

        // Build display blocks: messages render standalone; runs of consecutive
        // activity items (commands/reasoning/tools/files) coalesce into one
        // collapsible group, like the codex desktop transcript.
        let mut rows: Vec<Div> = Vec::new();
        let mut group: Vec<&TimelineItem> = Vec::new();
        for it in &self.items {
            if it.kind == TimelineKind::Reasoning && it.text.is_empty() {
                continue; // skip empty reasoning placeholders
            }
            let is_msg = matches!(
                it.kind,
                TimelineKind::UserPrompt | TimelineKind::AssistantMessage
            );
            if is_msg {
                if !group.is_empty() {
                    rows.push(self.render_activity_block(&group, pal, mono.clone(), cx));
                    group.clear();
                }
                rows.push(self.render_row(it, pal, mono.clone(), cx));
            } else {
                group.push(it);
            }
        }
        if !group.is_empty() {
            rows.push(self.render_activity_block(&group, pal, mono.clone(), cx));
        }
        let approvals: Vec<Div> = self
            .pending_approvals
            .iter()
            .map(|req| self.render_approval(req, pal, cx))
            .collect();

        div()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(
                div()
                    .id("agent-chat-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll)
                    .p(px(12.0))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .when(rows.is_empty() && approvals.is_empty(), |this| {
                        this.child(
                            div()
                                .text_size(rems(0.82))
                                .text_color(pal.muted)
                                .child("发一条消息开始 Codex 会话。"),
                        )
                    })
                    .children(rows)
                    .children(approvals)
                    .when(self.show_working(), |this| {
                        let secs = self
                            .turn_started
                            .map(|t| t.elapsed().as_secs())
                            .unwrap_or(0);
                        let elapsed = if secs >= 60 {
                            format!("{}m {}s", secs / 60, secs % 60)
                        } else {
                            format!("{secs}s")
                        };
                        let tokens = self.last_usage.as_ref().map(|u| u.output_tokens).unwrap_or(0);
                        let meta = if tokens > 0 {
                            format!("已执行 {elapsed} · ↓ {}", fmt_tokens(tokens))
                        } else {
                            format!("已执行 {elapsed}")
                        };
                        this.child(render_working(pal, self.working_word(), meta, secs % 2 == 0))
                    }),
            )
            .child(self.render_composer(pal, input_bg, &input_value, cx))
    }
}

impl ChatView {
    fn render_row(
        &self,
        item: &TimelineItem,
        pal: Pal,
        mono: SharedString,
        cx: &mut Context<Self>,
    ) -> Div {
        let time = self.item_times.get(&item.id).cloned().unwrap_or_default();
        match item.kind {
            TimelineKind::UserPrompt => self.render_user_message(item, &time, pal, cx),
            TimelineKind::AssistantMessage => self.render_assistant_message(item, &time, pal, cx),
            _ => self.render_activity_card(item, pal, mono, false, cx),
        }
    }

    fn render_user_message(
        &self,
        item: &TimelineItem,
        time: &SharedString,
        pal: Pal,
        cx: &mut Context<Self>,
    ) -> Div {
        // If this message is being edited, render the inline editor instead.
        if let Some((eid, input)) = &self.editing
            && eid == &item.id
        {
            return self.render_user_edit(input.clone(), pal, cx);
        }

        let text = item.text.clone();
        let copy_text = text.clone();
        let edit_text = text.clone();
        let edit_id = item.id.clone();
        let group = SharedString::from(format!("m-{}", item.id));
        div()
            .flex()
            .w_full()
            .justify_end()
            .child(
                div()
                    .group(group.clone())
                    .flex()
                    .flex_col()
                    .items_end()
                    .gap_1()
                    .max_w(relative_w())
                    .child(
                        div()
                            .rounded(px(12.0))
                            .px(px(12.0))
                            .py(px(8.0))
                            .bg(pal.bubble)
                            .text_size(rems(0.85))
                            .text_color(pal.fg)
                            .child(text),
                    )
                    .child(
                        meta_row(time.clone(), pal)
                            .opacity(0.0)
                            .group_hover(group, |s| s.opacity(1.0))
                            .child(icon_action(
                                SharedString::from(format!("copy-{}", item.id)),
                                HeroIconName::ClipboardDocument,
                                pal,
                                cx.listener(move |_, _, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(copy_text.clone()));
                                }),
                            ))
                            .child(icon_action(
                                SharedString::from(format!("edit-{}", item.id)),
                                HeroIconName::PencilSquare,
                                pal,
                                cx.listener(move |view, _, window, cx| {
                                    view.begin_edit(edit_id.clone(), edit_text.clone(), window, cx);
                                }),
                            )),
                    ),
            )
    }

    fn render_user_edit(
        &self,
        input: Entity<InputState>,
        pal: Pal,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex()
            .w_full()
            .justify_end()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .w_full()
                    .max_w(relative_w())
                    .child(
                        div()
                            .rounded(px(12.0))
                            .p(px(8.0))
                            .bg(pal.secondary)
                            .border_1()
                            .border_color(pal.primary)
                            .child(
                                div()
                                    .max_h(px(160.0))
                                    .child(Input::new(&input).appearance(false)),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_end()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(rems(0.62))
                                    .text_color(pal.muted)
                                    .child("回车保存并重发，会清除后续消息"),
                            )
                            .child(
                                Button::new("edit-cancel")
                                    .ghost()
                                    .with_size(Size::Small)
                                    .child("取消")
                                    .on_click(cx.listener(|view, _e, _w, cx| view.cancel_edit(cx))),
                            )
                            .child(
                                Button::new("edit-resend")
                                    .primary()
                                    .with_size(Size::Small)
                                    .child("重发")
                                    .on_click(cx.listener(|view, _e, window, cx| {
                                        view.commit_edit(window, cx)
                                    })),
                            ),
                    ),
            )
    }

    fn render_assistant_message(
        &self,
        item: &TimelineItem,
        time: &SharedString,
        pal: Pal,
        cx: &mut Context<Self>,
    ) -> Div {
        let text = item.text.clone();
        let copy_text = text.clone();
        let group = SharedString::from(format!("m-{}", item.id));
        // Assistant messages span the full column width (like codex/claude) so
        // markdown wraps at the panel width instead of collapsing to min-content.
        div()
            .group(group.clone())
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .gap_1()
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .text_size(rems(0.85))
                    .text_color(pal.fg)
                    .child(TextView::markdown(
                        SharedString::from(format!("md-{}", item.id)),
                        text,
                    )),
            )
            .child(
                meta_row(time.clone(), pal)
                    .opacity(0.0)
                    .group_hover(group, |s| s.opacity(1.0))
                    .child(icon_action(
                        SharedString::from(format!("copy-{}", item.id)),
                        HeroIconName::ClipboardDocument,
                        pal,
                        cx.listener(move |_, _, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copy_text.clone()));
                        }),
                    )),
            )
    }

    fn render_approval(&self, req: &ApprovalRequest, pal: Pal, cx: &mut Context<Self>) -> Div {
        let token_accept = req.token.clone();
        let token_decline = req.token.clone();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .rounded(px(8.0))
            .p(px(10.0))
            .border_1()
            .border_color(pal.primary)
            .child(
                div()
                    .text_size(rems(0.72))
                    .text_color(pal.primary)
                    .child(format!("需要批准 · {}", req.method)),
            )
            .child(div().text_size(rems(0.82)).text_color(pal.fg).child(req.summary.clone()))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new(SharedString::from(format!("approve-{}", req.token)))
                            .primary()
                            .with_size(Size::Small)
                            .child("批准")
                            .on_click(cx.listener(move |view, _e, _w, cx| {
                                view.respond_approval(token_accept.clone(), ApprovalDecision::Accept, cx)
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("decline-{}", req.token)))
                            .ghost()
                            .with_size(Size::Small)
                            .text_color(pal.danger)
                            .child("拒绝")
                            .on_click(cx.listener(move |view, _e, _w, cx| {
                                view.respond_approval(token_decline.clone(), ApprovalDecision::Decline, cx)
                            })),
                    ),
            )
    }

    fn render_composer(
        &self,
        pal: Pal,
        input_bg: gpui::Hsla,
        input_value: &str,
        cx: &mut Context<Self>,
    ) -> Div {
        let access = self.access;
        let access_color = if access == Access::FullAccess { pal.danger } else { pal.muted };
        let model_label = self
            .model
            .as_deref()
            .and_then(|id| self.models.iter().find(|m| m.id == id))
            .map(|m| m.display_name.clone())
            .or_else(|| self.model.clone())
            .unwrap_or_else(|| "默认模型".into());
        let effort_label = self.effort.clone();
        let plus_entity = cx.entity();

        // The @-mention picker preempts the `/` palette while a mention is active.
        let show_mention = self.mention_query.is_some();
        let show_slash = !show_mention && input_value.starts_with('/');

        let chips: Vec<Div> = self
            .attachments
            .iter()
            .enumerate()
            .map(|(i, a)| self.render_attachment_chip(i, a, pal, cx))
            .collect();

        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap_2()
            .p(px(10.0))
            .when(show_slash, |this| this.child(self.render_slash_menu(input_value, pal, cx)))
            .when(show_mention, |this| this.child(self.render_mention_menu(pal, cx)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .rounded(px(12.0))
                    .p(px(8.0))
                    .bg(input_bg)
                    .border_1()
                    .border_color(pal.border)
                    .when(!chips.is_empty(), |this| {
                        this.child(div().flex().flex_wrap().gap_1().children(chips))
                    })
                    .child(
                        div()
                            .max_h(px(180.0))
                            .child(Input::new(&self.input).appearance(false)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            // Left cluster: add-context (+) and access mode.
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(
                                        Button::new("composer-plus")
                                            .ghost()
                                            .with_size(Size::Small)
                                            .child(Icon::new(HeroIconName::Plus).size_4())
                                            .dropdown_menu(move |menu, _window, _cx| {
                                                let e1 = plus_entity.clone();
                                                let e2 = plus_entity.clone();
                                                menu.item(
                                                    PopupMenuItem::new("添加文件 (@)")
                                                        .icon(HeroIconName::Document)
                                                        .on_click(move |_, window, cx| {
                                                            cx.update_entity(&e1, |view, cx| {
                                                                let v = format!(
                                                                    "{}@",
                                                                    view.input.read(cx).value()
                                                                );
                                                                view.set_input(&v, window, cx);
                                                                view.on_input_changed(cx);
                                                            });
                                                        }),
                                                )
                                                .item(
                                                    PopupMenuItem::new("添加图片")
                                                        .icon(HeroIconName::Photo)
                                                        .on_click(move |_, _window, cx| {
                                                            cx.update_entity(&e2, |view, cx| {
                                                                view.pick_images(cx)
                                                            });
                                                        }),
                                                )
                                            }),
                                    )
                                    .child(self.access_button(access, access_color, cx)),
                            )
                            // Right cluster: model, effort, send.
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(self.model_button(model_label, pal, cx))
                                    .child(self.effort_button(effort_label, pal, cx))
                                    .child(self.send_button(pal, cx)),
                            ),
                    ),
            )
    }

    fn render_attachment_chip(
        &self,
        idx: usize,
        att: &Attachment,
        pal: Pal,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .gap_1()
            .px(px(6.0))
            .py(px(2.0))
            .rounded(px(6.0))
            .bg(pal.secondary)
            .child(Icon::new(att.icon()).size_3().text_color(pal.muted))
            .child(
                div()
                    .max_w(px(160.0))
                    .truncate()
                    .text_size(rems(0.72))
                    .text_color(pal.fg)
                    .child(att.label()),
            )
            .child(icon_action(
                SharedString::from(format!("chip-x-{idx}")),
                HeroIconName::XMark,
                pal,
                cx.listener(move |view, _e, _w, cx| view.remove_attachment(idx, cx)),
            ))
    }

    fn render_mention_menu(&self, pal: Pal, cx: &mut Context<Self>) -> impl IntoElement {
        let mut menu = div()
            .id("agent-mention-menu")
            .flex()
            .flex_col()
            .max_h(px(280.0))
            .overflow_y_scroll()
            .rounded(px(10.0))
            .border_1()
            .border_color(pal.border)
            .bg(pal.secondary)
            .p(px(4.0));
        if self.mention_hits.is_empty() {
            return menu.child(
                div()
                    .p(px(8.0))
                    .text_size(rems(0.75))
                    .text_color(pal.muted)
                    .child("输入以搜索文件…"),
            );
        }
        for hit in self.mention_hits.iter().take(20) {
            let h = hit.clone();
            menu = menu.child(slash_row(
                SharedString::from(format!("mention-{}", hit.path)),
                Some(HeroIconName::Document),
                hit.file_name.clone(),
                hit.path.clone(),
                pal,
                cx.listener(move |view, _e, window, cx| view.add_mention(h.clone(), window, cx)),
            ));
        }
        menu
    }

    fn access_button(
        &self,
        access: Access,
        color: gpui::Hsla,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity();
        // Prefer the server's permission profiles; fall back to the built-in three
        // (which are exactly codex's defaults) before the catalog arrives.
        let opts: Vec<(Access, String)> = if self.profiles.is_empty() {
            vec![
                (Access::ReadOnly, Access::ReadOnly.label().to_string()),
                (Access::WorkspaceWrite, Access::WorkspaceWrite.label().to_string()),
                (Access::FullAccess, Access::FullAccess.label().to_string()),
            ]
        } else {
            self.profiles
                .iter()
                .map(|p| {
                    let a = Access::from_profile_id(&p.id);
                    let label = p.description.clone().unwrap_or_else(|| a.label().to_string());
                    (a, label)
                })
                .collect()
        };
        Button::new("composer-access")
            .ghost()
            .with_size(Size::Small)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_color(color)
                    .child(Icon::new(HeroIconName::ExclamationTriangle).size_3())
                    .child(div().text_size(rems(0.72)).child(access.label()))
                    .child(Icon::new(HeroIconName::ChevronDown).size_3()),
            )
            .dropdown_menu(move |menu, _window, _cx| {
                opts.clone().into_iter().fold(menu, |menu, (a, label)| {
                    let e = entity.clone();
                    menu.item(PopupMenuItem::new(label).on_click(move |_, _window, cx| {
                        cx.update_entity(&e, |view, cx| view.set_access(a, cx));
                    }))
                })
            })
            .into_any_element()
    }

    fn model_button(
        &self,
        label: String,
        pal: Pal,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity();
        let models = self.models.clone();
        let selected = self.model.clone();
        Button::new("composer-model")
            .ghost()
            .with_size(Size::Small)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_color(pal.muted)
                    .child(Icon::new(HeroIconName::CubeTransparent).size_3())
                    .child(div().text_size(rems(0.72)).child(label))
                    .child(Icon::new(HeroIconName::ChevronDown).size_3()),
            )
            .dropdown_menu(move |mut menu, _window, _cx| {
                if models.is_empty() {
                    // No catalog yet (session not started): offer the manual route.
                    let e = entity.clone();
                    return menu.item(
                        PopupMenuItem::new("自定义… (/model)").on_click(move |_, window, cx| {
                            cx.update_entity(&e, |view, cx| view.set_input("/model ", window, cx));
                        }),
                    );
                }
                for m in &models {
                    let e = entity.clone();
                    let id = m.id.clone();
                    let mark = if selected.as_deref() == Some(m.id.as_str()) {
                        "  ✓"
                    } else if selected.is_none() && m.is_default {
                        "  ✓"
                    } else {
                        ""
                    };
                    let title = format!("{}{}", m.display_name, mark);
                    menu = menu.item(PopupMenuItem::new(title).on_click(move |_, _w, cx| {
                        let id = id.clone();
                        cx.update_entity(&e, |view, cx| view.set_model_value(Some(id), cx));
                    }));
                }
                menu
            })
            .into_any_element()
    }

    fn effort_button(
        &self,
        label: SharedString,
        pal: Pal,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity();
        let efforts = self.current_efforts();
        Button::new("composer-effort")
            .ghost()
            .with_size(Size::Small)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_color(pal.muted)
                    .child(Icon::new(HeroIconName::CpuChip).size_3())
                    .child(div().text_size(rems(0.72)).child(label))
                    .child(Icon::new(HeroIconName::ChevronDown).size_3()),
            )
            .dropdown_menu(move |menu, _window, _cx| {
                efforts.clone().into_iter().fold(menu, |menu, level| {
                    let e = entity.clone();
                    menu.item(PopupMenuItem::new(level.clone()).on_click(move |_, _w, cx| {
                        let level = level.clone();
                        cx.update_entity(&e, |view, cx| view.set_effort_value(level, cx));
                    }))
                })
            })
            .into_any_element()
    }

    /// Reasoning efforts supported by the currently-selected model (falls back to
    /// the server default model, then a generic ladder if the catalog is empty).
    fn current_efforts(&self) -> Vec<String> {
        let chosen = self
            .model
            .as_deref()
            .and_then(|id| self.models.iter().find(|m| m.id == id))
            .or_else(|| self.models.iter().find(|m| m.is_default))
            .or_else(|| self.models.first());
        match chosen {
            Some(m) if !m.supported_efforts.is_empty() => m.supported_efforts.clone(),
            _ => ["minimal", "low", "medium", "high", "xhigh"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }

    fn send_button(&self, pal: Pal, cx: &mut Context<Self>) -> impl IntoElement {
        // Two states: idle = send (arrow), in-flight = stop (square).
        let busy = self.busy;
        let (icon, bg) = if busy {
            (HeroIconName::Stop, pal.danger)
        } else {
            (HeroIconName::ArrowUp, pal.primary)
        };
        div()
            .id("composer-send")
            .size(px(30.0))
            .rounded_full()
            .bg(bg)
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|s| s.opacity(0.85))
            .on_click(cx.listener(move |view, _e, window, cx| {
                if busy {
                    view.stop(cx);
                } else {
                    view.submit(window, cx);
                }
            }))
            .child(
                Icon::new(icon)
                    .size_4()
                    .text_color(pal.primary_fg),
            )
    }

    fn render_slash_menu(&self, filter: &str, pal: Pal, cx: &mut Context<Self>) -> impl IntoElement {
        // The query after the leading `/` (e.g. "/com" -> "com").
        let term = filter.trim().trim_start_matches('/').to_lowercase();
        let cmd_matches: Vec<Command> = commands()
            .into_iter()
            .filter(|c| term.is_empty() || c.name[1..].to_lowercase().starts_with(&term))
            .collect();
        // Skills come from the server (`skills/list`), not a hardcoded list.
        let skill_matches: Vec<&AgentSkill> = self
            .skills
            .iter()
            .filter(|s| term.is_empty() || s.name.to_lowercase().contains(&term))
            .collect();

        let mut menu = div()
            .id("agent-slash-menu")
            .flex()
            .flex_col()
            .max_h(px(280.0))
            .overflow_y_scroll()
            .rounded(px(10.0))
            .border_1()
            .border_color(pal.border)
            .bg(pal.secondary)
            .p(px(4.0));

        if cmd_matches.is_empty() && skill_matches.is_empty() {
            return menu.child(
                div()
                    .p(px(8.0))
                    .text_size(rems(0.75))
                    .text_color(pal.muted)
                    .child("无匹配命令"),
            );
        }

        for c in cmd_matches {
            let token = c.token;
            menu = menu.child(
                slash_row(
                    SharedString::from(format!("slash-{}", c.name)),
                    Some(c.icon),
                    c.name.to_string(),
                    c.desc.to_string(),
                    pal,
                    cx.listener(move |view, _e, window, cx| {
                        view.command_clicked(token, window, cx)
                    }),
                ),
            );
        }

        if !skill_matches.is_empty() {
            menu = menu.child(
                div()
                    .px(px(8.0))
                    .pt(px(6.0))
                    .pb(px(2.0))
                    .text_size(rems(0.62))
                    .text_color(pal.muted)
                    .child("技能 (skills)"),
            );
            for s in skill_matches {
                let name = s.name.clone();
                let path = s.path.clone();
                menu = menu.child(slash_row(
                    SharedString::from(format!("skill-{}", s.name)),
                    Some(HeroIconName::Sparkles),
                    s.name.clone(),
                    s.description.clone(),
                    pal,
                    cx.listener(move |view, _e, window, cx| {
                        view.add_skill(name.clone(), path.clone(), window, cx);
                    }),
                ));
            }
        }
        menu
    }
}

fn relative_w() -> gpui::DefiniteLength {
    gpui::relative(0.9)
}

/// One row in the `/` palette: icon + name + (truncated) description.
fn slash_row(
    id: SharedString,
    icon: Option<HeroIconName>,
    name: String,
    desc: String,
    pal: Pal,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    let desc = if desc.chars().count() > 84 {
        let mut s: String = desc.chars().take(84).collect();
        s.push('…');
        s
    } else {
        desc
    };
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_2()
        .rounded(px(6.0))
        .px(px(8.0))
        .py(px(6.0))
        .cursor_pointer()
        .hover(|s| s.bg(pal.bubble))
        .on_click(on_click)
        .when_some(icon, |this, ic| {
            this.child(Icon::new(ic).size_4().text_color(pal.muted))
        })
        .child(div().flex_none().text_size(rems(0.8)).text_color(pal.fg).child(name))
        .when(!desc.is_empty(), |this| {
            this.child(
                div()
                    .min_w_0()
                    .text_size(rems(0.72))
                    .text_color(pal.muted)
                    .child(desc),
            )
        })
}

/// The active trailing `@mention` query (text after a leading-`@` last token),
/// or None if the caret isn't in a mention.
fn active_mention(value: &str) -> Option<String> {
    let last = value
        .rsplit(|c: char| c.is_whitespace())
        .next()
        .unwrap_or("");
    last.strip_prefix('@').map(|q| q.to_string())
}

/// Drop the active trailing `@…` token from the input text.
fn strip_active_mention(value: &str) -> String {
    match value.rfind('@') {
        Some(idx) => value[..idx].to_string(),
        None => value.to_string(),
    }
}

fn fmt_tokens(t: u64) -> String {
    if t >= 1000 {
        format!("{:.1}k tokens", t as f64 / 1000.0)
    } else {
        format!("{t} tokens")
    }
}

/// Working indicator shown for the whole turn: theme-color verb + elapsed time /
/// token meta (codex CLI style, e.g. "执行命令… 已执行 2m 18s · ↓ 5.8k"). Updated
/// once a second by the heartbeat — intentionally NOT a 60fps animation, which
/// would force the whole transcript to re-render every frame.
fn render_working(pal: Pal, word: &str, meta: String, pulse: bool) -> Div {
    div()
        .flex()
        .w_full()
        .items_center()
        .gap_2()
        .py(px(4.0))
        // A small dot that blinks once a second (cheap, no per-frame animation).
        .child(
            div()
                .size(px(7.0))
                .rounded_full()
                .bg(pal.primary)
                .opacity(if pulse { 1.0 } else { 0.4 }),
        )
        .child(
            div()
                .text_size(rems(0.85))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(pal.primary)
                .child(format!("{word}…")),
        )
        .child(div().text_size(rems(0.74)).text_color(pal.muted).child(meta))
}

fn meta_row(time: SharedString, pal: Pal) -> Div {
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(div().text_size(rems(0.65)).text_color(pal.muted).child(time))
}

fn icon_action(
    id: SharedString,
    icon: HeroIconName,
    pal: Pal,
    on_click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .p(px(2.0))
        .rounded(px(4.0))
        .cursor_pointer()
        .text_color(pal.muted)
        .hover(|s| s.bg(pal.bubble))
        .on_click(on_click)
        .child(Icon::new(icon).size_3())
}

impl ChatView {
    /// Render a run of consecutive activity items. A single item is just its card;
    /// multiple coalesce into one collapsible group ("已执行 N 步"), open while
    /// running and collapsed once done (codex-desktop style), toggleable.
    fn render_activity_block(
        &self,
        items: &[&TimelineItem],
        pal: Pal,
        mono: SharedString,
        cx: &mut Context<Self>,
    ) -> Div {
        // A run made entirely of images renders as a thumbnail grid (codux.app).
        let imgs: Vec<(&TimelineItem, String)> = items
            .iter()
            .filter_map(|it| image_path(it).map(|p| (*it, p)))
            .collect();
        if !imgs.is_empty() && imgs.len() == items.len() {
            let mut grid = div().flex().flex_wrap().gap_2();
            for (it, p) in imgs {
                grid = grid.child(self.render_image(it, p, pal, cx));
            }
            return grid;
        }
        if items.len() == 1 {
            return self.render_activity_card(items[0], pal, mono, false, cx);
        }
        let key = items[0].id.clone();
        let running = items.iter().any(|it| it.status == ItemStatus::InProgress);
        let failed = items.iter().any(|it| it.status == ItemStatus::Failed);
        let open = self.group_open.get(&key).copied().unwrap_or(running);

        let status: AnyElement = if running {
            div()
                .size(px(7.0))
                .rounded_full()
                .bg(pal.primary)
                .into_any_element()
        } else if failed {
            Icon::new(HeroIconName::ExclamationTriangle)
                .size_3()
                .text_color(pal.danger)
                .into_any_element()
        } else {
            Icon::new(HeroIconName::Check).size_3().text_color(pal.muted).into_any_element()
        };
        let summary = if running {
            format!("执行中… {} 步", items.len())
        } else {
            format!("已执行 {} 步", items.len())
        };
        let key2 = key.clone();
        let header = div()
            .id(SharedString::from(format!("grp-{key}")))
            .flex()
            .items_center()
            .gap_2()
            .px(px(8.0))
            .py(px(6.0))
            .cursor_pointer()
            .hover(|s| s.bg(pal.bubble))
            .on_click(cx.listener(move |view, _e, _w, cx| {
                view.toggle_group(key2.clone(), open, cx)
            }))
            .child(status)
            .child(Icon::new(HeroIconName::Bars3).size_3().text_color(pal.muted))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(rems(0.72))
                    .text_color(pal.fg)
                    .child(summary),
            )
            .child(
                Icon::new(if open {
                    HeroIconName::ChevronDown
                } else {
                    HeroIconName::ChevronRight
                })
                .size_3()
                .text_color(pal.muted),
            );

        let mut block = div()
            .flex()
            .flex_col()
            .rounded(px(8.0))
            .border_1()
            .border_color(pal.border)
            .overflow_hidden()
            .child(header);
        if open {
            let mut body = div().flex().flex_col().px(px(4.0)).pb(px(4.0));
            for it in items {
                body = body.child(self.render_activity_card(it, pal, mono.clone(), true, cx));
            }
            block = block.child(body);
        }
        block
    }

    /// Resolve a possibly-relative path against the session worktree.
    fn abs_path(&self, p: &str) -> std::path::PathBuf {
        let path = std::path::Path::new(p);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::path::Path::new(&self.cwd).join(path)
        }
    }

    /// An image item (generated/viewed): clickable thumbnail that opens with the
    /// system viewer, with its file name beneath.
    fn render_image(
        &self,
        item: &TimelineItem,
        path: String,
        pal: Pal,
        cx: &mut Context<Self>,
    ) -> Div {
        let abs = self.abs_path(&path);
        let abs_open = abs.clone();
        let name = std::path::Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .id(SharedString::from(format!("img-{}", item.id)))
                    .rounded(px(8.0))
                    .overflow_hidden()
                    .border_1()
                    .border_color(pal.border)
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.85))
                    .on_click(cx.listener(move |_v, _e, _w, cx| {
                        cx.open_with_system(&abs_open);
                    }))
                    .child(img(abs).max_w(px(220.0)).max_h(px(220.0))),
            )
            .child(
                div()
                    .max_w(px(220.0))
                    .truncate()
                    .text_size(rems(0.66))
                    .text_color(pal.muted)
                    .child(name),
            )
    }

    /// One activity item as a compact, single-line card that expands on click.
    /// `nested` drops the outer border (it sits inside a group).
    fn render_activity_card(
        &self,
        item: &TimelineItem,
        pal: Pal,
        mono: SharedString,
        nested: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        // Images render as a thumbnail, not a text card.
        if let Some(p) = image_path(item) {
            return self.render_image(item, p, pal, cx);
        }
        let (label, icon) = match item.kind {
            TimelineKind::Reasoning => ("思考", HeroIconName::LightBulb),
            TimelineKind::Command => ("命令", HeroIconName::CommandLine),
            TimelineKind::FileChange => ("文件", HeroIconName::DocumentText),
            TimelineKind::Plan => ("计划", HeroIconName::Bars3),
            _ => ("工具", HeroIconName::Cog6Tooth),
        };
        let is_cmd = item.kind == TimelineKind::Command;
        let is_reasoning = item.kind == TimelineKind::Reasoning;
        let is_filechange = item.kind == TimelineKind::FileChange;
        let changes = if is_filechange { file_changes(item) } else { Vec::new() };
        let (agg_add, agg_del) = changes
            .iter()
            .map(|(_, d)| diff_stats(d))
            .fold((0usize, 0usize), |(a, b), (c, d)| (a + c, b + d));

        // Single-line summary for the collapsed header.
        let primary = match item.kind {
            TimelineKind::Command => item.command.clone().unwrap_or_else(|| item.title.clone()),
            TimelineKind::Reasoning => first_line(&item.text),
            TimelineKind::FileChange => filechange_summary(&changes),
            _ if !item.title.is_empty() => item.title.clone(),
            _ => first_line(&item.text),
        };
        let body_text = if is_cmd || is_filechange { String::new() } else { item.text.clone() };
        let has_body = !changes.is_empty()
            || !item.output.is_empty()
            || (is_reasoning && (item.text.lines().count() > 1 || item.text.chars().count() > 64))
            || (!is_cmd && !is_reasoning && !is_filechange && !item.text.is_empty());
        let expanded = self.expanded.contains(&item.id);
        let id = item.id.clone();

        let status: AnyElement = match item.status {
            ItemStatus::InProgress => div()
                .size(px(7.0))
                .rounded_full()
                .bg(pal.primary)
                .into_any_element(),
            ItemStatus::Completed => Icon::new(HeroIconName::Check)
                .size_3()
                .text_color(pal.muted)
                .into_any_element(),
            ItemStatus::Failed => Icon::new(HeroIconName::ExclamationTriangle)
                .size_3()
                .text_color(pal.danger)
                .into_any_element(),
        };

        let header = div()
            .id(SharedString::from(format!("card-{id}")))
            .flex()
            .items_center()
            .gap_2()
            .px(px(8.0))
            .py(px(6.0))
            .when(has_body, |s| {
                let id2 = id.clone();
                s.cursor_pointer()
                    .hover(|s| s.bg(pal.bubble))
                    .on_click(cx.listener(move |view, _e, _w, cx| {
                        view.toggle_expand(id2.clone(), cx)
                    }))
            })
            .child(status)
            .child(Icon::new(icon).size_3().text_color(pal.muted))
            .child(div().flex_none().text_size(rems(0.66)).text_color(pal.muted).child(label))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(rems(if is_cmd { 0.72 } else { 0.78 }))
                    .text_color(pal.fg)
                    .when(is_cmd, |s| s.font_family(mono.clone()))
                    .child(primary),
            )
            .when(is_filechange && (agg_add > 0 || agg_del > 0), |s| {
                s.child(diff_badges(agg_add, agg_del, pal))
            })
            .when(has_body, |s| {
                let chev = if expanded {
                    HeroIconName::ChevronDown
                } else {
                    HeroIconName::ChevronRight
                };
                s.child(Icon::new(chev).size_3().text_color(pal.muted))
            });

        let mut card = div()
            .flex()
            .flex_col()
            .rounded(px(8.0))
            .overflow_hidden()
            .when(!nested, |s| s.border_1().border_color(pal.border))
            .child(header);

        if expanded && has_body {
            let mut body = div()
                .id(SharedString::from(format!("card-body-{id}")))
                .max_h(px(280.0))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_1()
                .px(px(8.0))
                .pb(px(8.0));
            if !body_text.is_empty() {
                body = body.child(
                    div()
                        .text_size(rems(0.78))
                        .text_color(if is_reasoning { pal.muted } else { pal.fg })
                        .child(body_text),
                );
            }
            // File changes: a row per file (path + ±stats), click to reveal its
            // diff — codux.app style.
            for (path, diff) in &changes {
                let (a, d) = diff_stats(diff);
                let fkey = format!("{}::{}", item.id, path);
                let fopen = self.expanded.contains(&fkey);
                let fkey2 = fkey.clone();
                let abs = self.abs_path(path);
                body = body.child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .py(px(3.0))
                        // Left: click to expand this file's diff.
                        .child(
                            div()
                                .id(SharedString::from(format!("frow-{fkey}")))
                                .flex()
                                .flex_1()
                                .min_w_0()
                                .items_center()
                                .gap_2()
                                .cursor_pointer()
                                .rounded(px(4.0))
                                .hover(|s| s.bg(pal.bubble))
                                .on_click(cx.listener(move |view, _e, _w, cx| {
                                    view.toggle_expand(fkey2.clone(), cx)
                                }))
                                .child(
                                    Icon::new(if fopen {
                                        HeroIconName::ChevronDown
                                    } else {
                                        HeroIconName::ChevronRight
                                    })
                                    .size_3()
                                    .text_color(pal.muted),
                                )
                                .child(
                                    Icon::new(HeroIconName::DocumentText)
                                        .size_3()
                                        .text_color(pal.muted),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .font_family(mono.clone())
                                        .text_size(rems(0.72))
                                        .text_color(pal.fg)
                                        .child(short_path(path)),
                                )
                                .child(diff_badges(a, d, pal)),
                        )
                        // Right: "打开方式 ▼" — open with system app or reveal in Finder.
                        .child(
                            Button::new(SharedString::from(format!("fopen-{fkey}")))
                                .ghost()
                                .with_size(Size::Small)
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .text_color(pal.muted)
                                        .child(div().text_size(rems(0.68)).child("打开方式"))
                                        .child(Icon::new(HeroIconName::ChevronDown).size_3()),
                                )
                                .dropdown_menu(move |menu, _window, _cx| {
                                    let open = abs.clone();
                                    let reveal = abs.clone();
                                    menu.item(
                                        PopupMenuItem::new("打开")
                                            .icon(HeroIconName::ArrowTopRightOnSquare)
                                            .on_click(move |_, _w, cx| cx.open_with_system(&open)),
                                    )
                                    .item(
                                        PopupMenuItem::new("在 Finder 中显示")
                                            .icon(HeroIconName::Folder)
                                            .on_click(move |_, _w, cx| cx.reveal_path(&reveal)),
                                    )
                                }),
                        ),
                );
                if fopen {
                    body = body.child(render_diff(diff, pal, mono.clone()));
                }
            }
            if !item.output.is_empty() {
                body = body.child(
                    div()
                        .font_family(mono)
                        .text_size(rems(0.7))
                        .text_color(pal.muted)
                        .child(item.output.trim_end().to_string()),
                );
            }
            card = card.child(body);
        }
        card
    }
}

fn first_line(s: &str) -> String {
    s.trim().lines().next().unwrap_or("").to_string()
}

/// If this item is an image (imageView / imageGeneration), return its file path.
fn image_path(item: &TimelineItem) -> Option<String> {
    match item.item_type.as_str() {
        "imageView" => item.raw.get("path").and_then(|v| v.as_str()).map(String::from),
        "imageGeneration" => item
            .raw
            .get("savedPath")
            .and_then(|v| v.as_str())
            .or_else(|| item.raw.get("result").and_then(|v| v.as_str()))
            .filter(|p| p.contains('/'))
            .map(String::from),
        _ => None,
    }
}

/// Extract (path, unified_diff) pairs from a fileChange item's raw `changes`.
fn file_changes(item: &TimelineItem) -> Vec<(String, String)> {
    item.raw
        .get("changes")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|c| {
                    let path = c.get("path").and_then(|p| p.as_str())?.to_string();
                    let diff = c
                        .get("diff")
                        .and_then(|d| d.as_str())
                        .or_else(|| c.get("unified_diff").and_then(|d| d.as_str()))
                        .unwrap_or_default()
                        .to_string();
                    Some((path, diff))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn filechange_summary(changes: &[(String, String)]) -> String {
    match changes {
        [] => "文件改动".into(),
        [(p, _)] => format!("编辑了 {}", short_path(p)),
        _ => format!("已编辑 {} 个文件", changes.len()),
    }
}

/// Last two path segments (codux.app shows compact paths).
fn short_path(path: &str) -> String {
    let parts: Vec<&str> = path.rsplit('/').take(2).collect();
    parts.into_iter().rev().collect::<Vec<_>>().join("/")
}

/// Count added / removed lines in a unified diff (ignoring +++/--- headers).
fn diff_stats(diff: &str) -> (usize, usize) {
    let mut add = 0;
    let mut del = 0;
    for l in diff.lines() {
        if l.starts_with("+++") || l.starts_with("---") {
            continue;
        }
        if l.starts_with('+') {
            add += 1;
        } else if l.starts_with('-') {
            del += 1;
        }
    }
    (add, del)
}

fn diff_green() -> gpui::Hsla {
    gpui::hsla(140.0 / 360.0, 0.5, 0.55, 1.0)
}

/// Green +N / red -M badges (codux.app diff stats).
fn diff_badges(add: usize, del: usize, pal: Pal) -> Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap_2()
        .text_size(rems(0.68))
        .when(add > 0, |s| {
            s.child(div().text_color(diff_green()).child(format!("+{add}")))
        })
        .when(del > 0, |s| {
            s.child(div().text_color(pal.danger).child(format!("-{del}")))
        })
}

/// Render a unified diff with +/- line coloring (green add, red remove).
fn render_diff(diff: &str, pal: Pal, mono: SharedString) -> Div {
    let add = diff_green();
    let del = pal.danger;
    let mut block = div().flex().flex_col().font_family(mono).text_size(rems(0.7));
    for line in diff.lines() {
        let color = if line.starts_with("@@") {
            pal.primary
        } else if line.starts_with('+') && !line.starts_with("+++") {
            add
        } else if line.starts_with('-') && !line.starts_with("---") {
            del
        } else {
            pal.muted
        };
        block = block.child(div().text_color(color).child(line.to_string()));
    }
    block
}

/// Multi-tab host for chat sessions bound to a single worktree. Each tab is its
/// own [`ChatView`] (its own codex session); the worktree-keyed entity lives in
/// `WorkspaceBodyView`, so switching projects/worktrees rebuilds the whole panel
/// — exactly like the terminal's tab strip but for AI conversations.
pub(in crate::app) struct ChatPanel {
    cwd: String,
    codex_program: String,
    tabs: Vec<Entity<ChatView>>,
    active: usize,
}

impl ChatPanel {
    pub(in crate::app) fn new(
        cwd: String,
        codex_program: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let first = Self::make_tab(&cwd, &codex_program, window, cx);
        Self {
            cwd,
            codex_program,
            tabs: vec![first],
            active: 0,
        }
    }

    fn make_tab(
        cwd: &str,
        codex_program: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ChatView> {
        let cwd = cwd.to_string();
        let program = codex_program.to_string();
        let tab = cx.new(|cx| ChatView::new(cwd, program, window, cx));
        // A /new from inside a tab opens another tab in this panel.
        cx.subscribe_in(&tab, window, |panel, _tab, event, window, cx| match event {
            ChatViewEvent::NewTab => panel.add_tab(window, cx),
        })
        .detach();
        tab
    }

    fn add_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab = Self::make_tab(&self.cwd, &self.codex_program, window, cx);
        self.tabs.push(tab);
        self.active = self.tabs.len() - 1;
        cx.notify();
    }

    fn select(&mut self, idx: usize, cx: &mut Context<Self>) {
        if idx < self.tabs.len() {
            self.active = idx;
            cx.notify();
        }
    }

    fn close_tab(&mut self, idx: usize, window: &mut Window, cx: &mut Context<Self>) {
        if idx >= self.tabs.len() {
            return;
        }
        self.tabs.remove(idx);
        if self.tabs.is_empty() {
            self.tabs
                .push(Self::make_tab(&self.cwd, &self.codex_program, window, cx));
            self.active = 0;
        } else if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        } else if idx < self.active {
            self.active -= 1;
        }
        cx.notify();
    }
}

impl Render for ChatPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let fg = theme.foreground;
        let secondary_fg = theme.secondary_foreground;
        let border = theme.border;
        let hover = theme.secondary_hover;
        let transparent = theme.transparent;
        let active = self.active;
        let multiple = self.tabs.len() > 1;

        let mut strip = div()
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .h(px(44.0))
            .px(px(8.0))
            .border_b_1()
            .border_color(border)
            .overflow_x_hidden();

        // Tabs match the terminal split's bottom tab style.
        for (i, tab) in self.tabs.iter().enumerate() {
            let is_active = i == active;
            let title = tab.read(cx).title();
            let mut chip = div()
                .id(SharedString::from(format!("chat-tab-{i}")))
                .h(px(30.0))
                .px_3()
                .flex()
                .items_center()
                .gap_2()
                .rounded_md()
                .cursor_pointer()
                .text_color(if is_active { fg } else { secondary_fg })
                .bg(if is_active { hover } else { transparent })
                .hover(|s| s.bg(hover))
                .on_click(cx.listener(move |panel, _e, _w, cx| panel.select(i, cx)))
                .child(
                    div()
                        .text_size(rems(0.75))
                        .line_height(rems(0.875))
                        .child(title),
                );
            if multiple {
                chip = chip.child(
                    div()
                        .id(SharedString::from(format!("chat-tab-close-{i}")))
                        .size(px(20.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_sm()
                        .text_color(secondary_fg)
                        .hover(|s| s.bg(hover))
                        .on_click(cx.listener(move |panel, _e, window, cx| {
                            cx.stop_propagation();
                            window.prevent_default();
                            panel.close_tab(i, window, cx)
                        }))
                        .child(Icon::new(HeroIconName::XMark).size_3()),
                );
            }
            strip = strip.child(chip);
        }

        strip = strip.child(
            div()
                .id("chat-tab-add")
                .size(px(26.0))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .rounded_sm()
                .cursor_pointer()
                .text_color(secondary_fg)
                .hover(|s| s.bg(hover))
                .on_click(cx.listener(|panel, _e, window, cx| panel.add_tab(window, cx)))
                .child(Icon::new(HeroIconName::Plus).size_3p5()),
        );

        let body = self
            .tabs
            .get(active)
            .cloned()
            .map(gpui::AnyView::from);

        div()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            // Match the terminal area's translucent backing.
            .bg(crate::theme::terminal_fill(crate::theme::color(
                crate::theme::BG_TERMINAL,
            )))
            .child(strip)
            .when_some(body, |this, view| {
                this.child(div().flex_1().min_h_0().child(view))
            })
    }
}
