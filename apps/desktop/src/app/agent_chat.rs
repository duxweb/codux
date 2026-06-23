//! GPUI chat view for a protocol-driven AI session (Codex).
//!
//! Renders the agent-driver's merged timeline as left/right message bubbles with
//! per-message actions, a Codex-style composer (access mode, model, effort,
//! round send button) and a `/` command menu. The session runs off-thread and is
//! bound to the active worktree; events arrive over a flume channel drained in a
//! `cx.spawn` loop so the UI thread never blocks.

use std::collections::HashMap;

use chrono::Local;
use codux_agent_driver::{
    AgentEvent, ApprovalDecision, ApprovalRequest, CodexAgentDriver, CodexModel,
    CodexPermissionProfile, CodexSession, CodexSkill, FileHit, ItemStatus, SessionConfig,
    TimelineItem, TimelineKind, TokenUsage, UserInputPart,
};
use flume::Sender;
use gpui::{
    AppContext, ClipboardItem, Context, Div, Entity, EventEmitter, InteractiveElement, IntoElement,
    ParentElement, PathPromptOptions, Render, ScrollHandle, SharedString,
    StatefulInteractiveElement, Styled, Task, WeakEntity, Window, div,
    prelude::FluentBuilder as _, px, rems,
};
use gpui_component::{
    ActiveTheme, Icon, Sizable, Size,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    menu::{DropdownMenu, PopupMenuItem},
    spinner::Spinner,
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
    Started(CodexSession),
    /// The server's dynamic catalog (models / skills / permission profiles),
    /// fetched after the session starts — these drive the composer dropdowns and
    /// the `/` palette instead of a hardcoded list.
    Catalog {
        models: Vec<CodexModel>,
        skills: Vec<CodexSkill>,
        profiles: Vec<CodexPermissionProfile>,
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
        let driver = CodexAgentDriver { program, env };
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
        match CodexSession::start(&driver, &cfg, sink) {
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
    session: Option<CodexSession>,
    starting: bool,
    /// A turn is in flight (drives the send/stop button state).
    busy: bool,
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
    models: Vec<CodexModel>,
    skills: Vec<CodexSkill>,
    profiles: Vec<CodexPermissionProfile>,
    /// Structured input parts staged for the next turn (skills, @-files, images).
    attachments: Vec<Attachment>,
    /// Last token usage seen, surfaced by /status.
    last_usage: Option<TokenUsage>,
    /// Active @-mention query and its current hits (drives the file picker).
    mention_query: Option<String>,
    mention_hits: Vec<FileHit>,
    tx: Sender<ChatMsg>,
    _drain: Task<()>,
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
            while let Ok(msg) = rx.recv_async().await {
                if this.update(cx, |view, cx| view.handle_msg(msg, cx)).is_err() {
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
            tx,
            _drain: drain,
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

    fn handle_msg(&mut self, msg: ChatMsg, cx: &mut Context<Self>) {
        match msg {
            ChatMsg::Note(note) => self.status = SharedString::from(note),
            ChatMsg::Started(session) => {
                // Flush anything composed while connecting.
                let busy = !self.pending_send.is_empty();
                for parts in self.pending_send.drain(..) {
                    let s = session.clone();
                    std::thread::spawn(move || {
                        let _ = s.send_user_turn(parts);
                    });
                }
                self.session = Some(session);
                self.starting = false;
                self.status = SharedString::from(if busy { "生成中…" } else { "就绪" });
            }
            ChatMsg::Catalog {
                models,
                skills,
                profiles,
            } => {
                // Default the model selection to the server's default if the user
                // hasn't picked one, and align effort to that model's default.
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
            }
            ChatMsg::FileHits { query, hits } => {
                // Ignore results for a query the user has since changed past.
                if self.mention_query.as_deref() == Some(query.as_str()) {
                    self.mention_hits = hits;
                }
            }
            ChatMsg::Failed(err) => {
                self.starting = false;
                self.busy = false;
                self.status = SharedString::from(format!("错误: {err}"));
            }
            ChatMsg::Event(ev) => {
                if let Some(session) = &self.session {
                    self.items = session.timeline_snapshot();
                }
                let now = Local::now().format("%H:%M").to_string();
                for item in &self.items {
                    self.item_times
                        .entry(item.id.clone())
                        .or_insert_with(|| SharedString::from(now.clone()));
                }
                match ev {
                    AgentEvent::ApprovalRequest(req) => self.pending_approvals.push(req),
                    AgentEvent::TurnCompleted => {
                        self.busy = false;
                        self.status = SharedString::from("就绪");
                    }
                    AgentEvent::TokenUsage(u) => self.last_usage = Some(u),
                    AgentEvent::Error(err) => {
                        self.busy = false;
                        self.status = SharedString::from(format!("错误: {err}"));
                    }
                    AgentEvent::Status(_) => {}
                    _ => self.status = SharedString::from("生成中…"),
                }
            }
        }
        self.scroll.scroll_to_bottom();
        cx.notify();
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

        self.busy = true;
        if let Some(session) = &self.session {
            let session = session.clone();
            std::thread::spawn(move || {
                let _ = session.send_user_turn(parts);
            });
            self.status = SharedString::from("生成中…");
        } else {
            // Session still connecting (or failed): queue, and (re)start if needed.
            self.pending_send.push(parts);
            self.ensure_session();
            self.status = SharedString::from("连接中…");
        }
        cx.notify();
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
        self.status = SharedString::from("已中断");
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

    /// Show the "thinking…" indicator while a turn is in flight, except while the
    /// assistant is already streaming visible text (the text itself is the cue).
    fn show_thinking(&self) -> bool {
        self.busy
            && !self.items.last().is_some_and(|it| {
                it.kind == TimelineKind::AssistantMessage
                    && !it.text.is_empty()
                    && it.status == ItemStatus::InProgress
            })
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

        let rows: Vec<Div> = self
            .items
            .iter()
            .filter(|it| {
                // Skip empty reasoning placeholders that have no text yet.
                !(it.kind == TimelineKind::Reasoning && it.text.is_empty())
            })
            .map(|item| self.render_row(item, pal, mono.clone(), cx))
            .collect();
        let approvals: Vec<Div> = self
            .pending_approvals
            .iter()
            .map(|req| self.render_approval(req, pal, cx))
            .collect();

        let worktree = std::path::Path::new(&self.cwd)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        div()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(34.0))
                    .px(px(12.0))
                    .border_b_1()
                    .border_color(pal.border)
                    .child(
                        div()
                            .text_size(rems(0.82))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(pal.fg)
                            .child("Codex"),
                    )
                    .when(!worktree.is_empty(), |this| {
                        this.child(div().text_size(rems(0.7)).text_color(pal.muted).child(worktree))
                    })
                    .child(div().flex_1())
                    .child(div().text_size(rems(0.7)).text_color(pal.muted).child(self.status.clone())),
            )
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
                    .when(self.show_thinking(), |this| {
                        this.child(render_thinking(pal))
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
            _ => render_activity_card(item, pal, mono),
        }
    }

    fn render_user_message(
        &self,
        item: &TimelineItem,
        time: &SharedString,
        pal: Pal,
        cx: &mut Context<Self>,
    ) -> Div {
        let text = item.text.clone();
        let copy_text = text.clone();
        let edit_text = text.clone();
        div()
            .flex()
            .w_full()
            .justify_end()
            .child(
                div()
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
                                    view.set_input(&edit_text, window, cx);
                                }),
                            )),
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
        div()
            .flex()
            .w_full()
            .justify_start()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap_1()
                    .max_w(relative_w())
                    .child(
                        div()
                            .text_size(rems(0.85))
                            .text_color(pal.fg)
                            .child(text),
                    )
                    .child(
                        meta_row(time.clone(), pal).child(icon_action(
                            SharedString::from(format!("copy-{}", item.id)),
                            HeroIconName::ClipboardDocument,
                            pal,
                            cx.listener(move |_, _, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copy_text.clone()));
                            }),
                        )),
                    ),
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
        let skill_matches: Vec<&CodexSkill> = self
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

/// Animated "thinking…" placeholder shown while the agent works.
fn render_thinking(pal: Pal) -> Div {
    div().flex().w_full().justify_start().child(
        div()
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(8.0))
            .px(px(10.0))
            .py(px(6.0))
            .child(Spinner::new().with_size(Size::Small).color(pal.muted))
            .child(
                div()
                    .text_size(rems(0.8))
                    .text_color(pal.muted)
                    .child("思考中…"),
            ),
    )
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

/// Reasoning / command / file-change / tool items render as subtle left cards.
fn render_activity_card(item: &TimelineItem, pal: Pal, mono: SharedString) -> Div {
    let (label, label_color) = match item.kind {
        TimelineKind::Reasoning => ("思考", pal.muted),
        TimelineKind::Command => ("命令", pal.fg),
        TimelineKind::FileChange => ("文件", pal.fg),
        TimelineKind::Plan => ("计划", pal.fg),
        _ => ("工具", pal.fg),
    };
    let status_mark = match item.status {
        ItemStatus::Completed => "",
        ItemStatus::Failed => " ✗",
        ItemStatus::InProgress => " …",
    };
    div()
        .flex()
        .flex_col()
        .gap_1()
        .rounded(px(8.0))
        .p(px(8.0))
        .border_1()
        .border_color(pal.border)
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_size(rems(0.68))
                .text_color(label_color)
                .child(format!("{label}{status_mark}"))
                .when(item.kind == TimelineKind::ToolCall && !item.title.is_empty(), |this| {
                    this.child(div().text_color(pal.muted).child(item.title.clone()))
                }),
        )
        .when(item.kind == TimelineKind::Command, |this| {
            this.child(
                div()
                    .font_family(mono.clone())
                    .text_size(rems(0.72))
                    .text_color(pal.fg)
                    .child(item.command.clone().unwrap_or_else(|| item.title.clone())),
            )
        })
        .when(!item.text.is_empty(), |this| {
            this.child(
                div()
                    .text_size(rems(0.78))
                    .text_color(if item.kind == TimelineKind::Reasoning { pal.muted } else { pal.fg })
                    .child(item.text.clone()),
            )
        })
        .when(!item.output.is_empty(), |this| {
            this.child(
                div()
                    .font_family(mono)
                    .text_size(rems(0.7))
                    .text_color(pal.muted)
                    .child(item.output.trim_end().to_string()),
            )
        })
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
        let muted = theme.muted_foreground;
        let border = theme.border;
        let secondary = theme.secondary;
        let active = self.active;
        let multiple = self.tabs.len() > 1;

        let mut strip = div()
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .h(px(32.0))
            .px(px(6.0))
            .border_b_1()
            .border_color(border)
            .overflow_x_hidden();

        for (i, tab) in self.tabs.iter().enumerate() {
            let is_active = i == active;
            let title = tab.read(cx).title();
            let mut chip = div()
                .id(SharedString::from(format!("chat-tab-{i}")))
                .flex()
                .items_center()
                .gap_1()
                .px(px(8.0))
                .py(px(3.0))
                .rounded(px(6.0))
                .cursor_pointer()
                .text_size(rems(0.74))
                .text_color(if is_active { fg } else { muted })
                .when(is_active, |s| s.bg(secondary))
                .when(!is_active, |s| s.hover(|s| s.bg(secondary)))
                .on_click(cx.listener(move |panel, _e, _w, cx| panel.select(i, cx)))
                .child(div().child(title));
            if multiple {
                chip = chip.child(
                    div()
                        .id(SharedString::from(format!("chat-tab-close-{i}")))
                        .p(px(1.0))
                        .rounded(px(3.0))
                        .text_color(muted)
                        .hover(|s| s.bg(border))
                        .on_click(cx.listener(move |panel, _e, window, cx| {
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
                .ml_1()
                .p(px(3.0))
                .rounded(px(4.0))
                .text_color(muted)
                .cursor_pointer()
                .hover(|s| s.bg(secondary))
                .on_click(cx.listener(|panel, _e, window, cx| panel.add_tab(window, cx)))
                .child(Icon::new(HeroIconName::Plus).size_4()),
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
            .child(strip)
            .when_some(body, |this, view| {
                this.child(div().flex_1().min_h_0().child(view))
            })
    }
}
