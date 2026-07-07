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
    ApprovalDecision, ApprovalRequest, FileHit, ItemStatus, PermissionRequest, PlanStep,
    SessionConfig, ThreadInfo, TimelineItem, TimelineKind, TokenUsage, UserInputPart,
    UserInputRequest, resume_session, start_session,
};
use flume::Sender;
use gpui::{
    AnyElement, AppContext, ClipboardItem, Context, Div, Entity, EventEmitter, FollowMode,
    InteractiveElement, IntoElement, ListAlignment, ListState, ParentElement, PathPromptOptions,
    Render, SharedString, StatefulInteractiveElement, Styled, Task, WeakEntity, Window, div, img,
    list, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme, ElementExt as _, Icon, Sizable, Size,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    menu::{DropdownMenu, PopupMenuItem},
    text::TextView,
};

use crate::heroicons::HeroIconName;

/// Chat panes live in the terminal split tree under their own id namespace
/// (like `gpui-term-`), so layout persistence/restore carries them for free.
pub(in crate::app) fn terminal_id_is_chat(terminal_id: &str) -> bool {
    terminal_id.starts_with("gpui-chat-")
}

fn unique_chat_terminal_id() -> String {
    format!("gpui-chat-{}", uuid::Uuid::new_v4())
}

impl crate::app::CoduxApp {
    /// Insert an AI chat pane into the terminal split tree at the picked
    /// direction/scope — a chat pane is one more split, only its content is a
    /// chat session instead of a PTY. (Codex-first; the agent picker gates
    /// other kinds until their drivers land.)
    pub(in crate::app) fn open_chat_split_direction(
        &mut self,
        direction: crate::app::TerminalSplitDirection,
        scope: crate::app::TerminalSplitScope,
        source_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::app::terminal_state::{
            terminal_split_tree_for_panes, terminal_split_tree_insert_pane,
            terminal_split_tree_insert_pane_root, terminal_top_grid_for_panes,
            terminal_top_ratios_for_panes,
        };
        use crate::app::{TerminalSplitDirection, TerminalSplitScope};
        let Some(active_tab) = self.main_terminal() else {
            return;
        };
        let pane_count = active_tab.panes.len();
        if pane_count >= codux_runtime::terminal_layout::TERMINAL_SPLIT_CAP {
            self.status_message = "main split limit reached".to_string();
            self.invalidate_terminal_workspace(cx);
            return;
        }
        let source_index = source_index.min(pane_count.saturating_sub(1));
        let top_ratios = terminal_top_ratios_for_panes(
            self.state.terminal_layout.top_ratios.clone(),
            pane_count,
        );
        let grid = terminal_top_grid_for_panes(
            self.state.terminal_layout.top_grid.clone(),
            &top_ratios,
            pane_count,
        );
        let tree = terminal_split_tree_for_panes(
            self.state.terminal_layout.split_tree.clone(),
            &grid,
            &top_ratios,
            pane_count,
        )
        .unwrap_or(codux_runtime::terminal_layout::TerminalSplitNode::Leaf { pane: 0 });
        let before = matches!(
            direction,
            TerminalSplitDirection::Left | TerminalSplitDirection::Up
        );
        let insert_index = match scope {
            TerminalSplitScope::Inner => {
                if before {
                    source_index
                } else {
                    source_index + 1
                }
            }
            TerminalSplitScope::Root => {
                if before {
                    0
                } else {
                    pane_count
                }
            }
        };
        let split_tree_result = match scope {
            TerminalSplitScope::Inner => {
                terminal_split_tree_insert_pane(&tree, source_index, insert_index, direction)
            }
            TerminalSplitScope::Root => {
                terminal_split_tree_insert_pane_root(&tree, insert_index, direction)
            }
        };
        let split_tree = match split_tree_result {
            Ok(result) => result,
            Err(error) => {
                self.status_message = error.to_string();
                self.invalidate_terminal_workspace(cx);
                return;
            }
        };
        let chat_id = unique_chat_terminal_id();
        let title = crate::app::workspace_shared::workspace_i18n(
            &self.state.settings.language,
            "terminal.chat.title",
            "AI Chat",
        );
        if let Some(tab) = self.main_terminal_mut() {
            let insert_index = insert_index.min(tab.panes.len());
            tab.panes.insert(
                insert_index,
                crate::app::types::TerminalPaneSlot {
                    title,
                    terminal_id: Some(chat_id.clone()),
                    pane: None,
                    restored_output_bytes: 0,
                    restored_output_tail: String::new(),
                },
            );
        }
        self.set_terminal_split_tree(Some(split_tree));
        self.ensure_chat_view(&chat_id, window, cx);
        self.sync_terminal_state_after_layout_change(cx);
        self.invalidate_terminal_workspace(cx);
    }

    /// Create the chat view for a chat pane id if it does not exist yet. Views
    /// are app-level so switching worktrees keeps conversations alive.
    pub(in crate::app) fn ensure_chat_view(
        &mut self,
        chat_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.chat_views.contains_key(chat_id) {
            return;
        }
        let Some(cwd) = self.selected_worktree_path() else {
            return;
        };
        let codex_program = self
            .runtime
            .root
            .join("scripts/wrappers/bin/codex")
            .to_string_lossy()
            .to_string();
        let app = cx.entity().downgrade();
        let view = cx.new(|cx| ChatView::new(app, cwd, codex_program, window, cx));
        // Pane title follows the CLI's own thread name (thread/name/updated).
        let title_chat_id = chat_id.to_string();
        cx.subscribe(&view, move |app, _view, event, cx| {
            let ChatViewEvent::TitleChanged(title) = event;
            app.set_chat_pane_title(&title_chat_id, title.clone(), cx);
        })
        .detach();
        self.chat_views.insert(chat_id.to_string(), view);
    }

    /// Rename the split pane that hosts `chat_id` (driven by thread/name/updated).
    fn set_chat_pane_title(&mut self, chat_id: &str, title: String, cx: &mut Context<Self>) {
        let title = title.trim().to_string();
        if title.is_empty() {
            return;
        }
        let mut changed = false;
        for tab in &mut self.terminals {
            for slot in &mut tab.panes {
                if slot.terminal_id.as_deref() == Some(chat_id) && slot.title != title {
                    slot.title = title.clone();
                    changed = true;
                }
            }
        }
        if changed {
            self.sync_terminal_state_after_layout_change(cx);
            self.invalidate_terminal_workspace(cx);
        }
    }

    /// Close a chat pane: remove it from the split tree and drop its session.
    pub(in crate::app) fn close_chat_pane(&mut self, pane_index: usize, cx: &mut Context<Self>) {
        use crate::app::terminal_state::{
            terminal_split_tree_for_panes, terminal_split_tree_remove_pane,
            terminal_top_grid_for_panes, terminal_top_ratios_for_panes,
        };
        let Some(tab_index) = (!self.terminals.is_empty()).then_some(0) else {
            return;
        };
        let Some(chat_id) = self.terminals[tab_index]
            .panes
            .get(pane_index)
            .and_then(|slot| slot.terminal_id.clone())
            .filter(|id| terminal_id_is_chat(id))
        else {
            return;
        };
        let pane_count = self.terminals[tab_index].panes.len();
        let top_ratios = terminal_top_ratios_for_panes(
            self.state.terminal_layout.top_ratios.clone(),
            pane_count,
        );
        let grid = terminal_top_grid_for_panes(
            self.state.terminal_layout.top_grid.clone(),
            &top_ratios,
            pane_count,
        );
        let tree = terminal_split_tree_for_panes(
            self.state.terminal_layout.split_tree.clone(),
            &grid,
            &top_ratios,
            pane_count,
        )
        .unwrap_or(codux_runtime::terminal_layout::TerminalSplitNode::Leaf { pane: 0 });
        let split_tree = terminal_split_tree_remove_pane(&tree, pane_index);
        self.terminals[tab_index].panes.remove(pane_index);
        self.set_terminal_split_tree(split_tree);
        self.chat_views.remove(&chat_id);
        // Closing the last pane leaves an empty grid; fall back to one
        // click-to-open terminal slot so the workspace never renders empty.
        if self.terminals[tab_index].panes.is_empty() {
            let title = self.text("terminal.title", "Terminal");
            self.terminals[tab_index]
                .panes
                .push(crate::app::types::TerminalPaneSlot {
                    title,
                    terminal_id: None,
                    pane: None,
                    restored_output_bytes: 0,
                    restored_output_tail: String::new(),
                });
            self.set_terminal_split_tree(Some(
                codux_runtime::terminal_layout::TerminalSplitNode::Leaf { pane: 0 },
            ));
        }
        self.sync_terminal_state_after_layout_change(cx);
        self.invalidate_terminal_workspace(cx);
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
        Command { icon: HeroIconName::Clock, name: "/resume", desc: "恢复本项目的历史会话", token: "/resume" },
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

/// One virtualized transcript row: a single message, or a run of consecutive
/// activity items, referencing `ChatView::items` by index range. `fingerprint`
/// captures everything height-affecting, so reconciliation splices (re-measures)
/// only rows that actually changed and the scroll position is preserved.
#[derive(Clone, PartialEq)]
struct Block {
    range: std::ops::Range<usize>,
    fingerprint: u64,
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
    /// Prior threads for this cwd (for the resume picker).
    Threads(Vec<ThreadInfo>),
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
    resume: Option<String>,
) {
    std::thread::spawn(move || {
        let _ = tx.send(ChatMsg::Note("Resolving codex…".into()));
        let (program, path_env) = resolve_codex(&wrapper);
        let _ = tx.send(ChatMsg::Note("Starting app-server…".into()));
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
        let _ = tx.send(ChatMsg::Note("Handshaking…".into()));
        // The driver kind is the single switch point for codex / claude / opencode.
        let started = match &resume {
            Some(tid) => resume_session(AgentKind::Codex, program, env, &cfg, tid, sink),
            None => start_session(AgentKind::Codex, program, env, &cfg, sink),
        };
        match started {
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

/// Events the app subscribes to (pane title follows the thread name).
pub(in crate::app) enum ChatViewEvent {
    TitleChanged(String),
}

impl EventEmitter<ChatViewEvent> for ChatView {}

pub(in crate::app) struct ChatView {
    /// Weak app handle for live settings reads (weak: the app owns chat views).
    app: WeakEntity<crate::app::CoduxApp>,
    /// Tracked on the root so the app's terminal key routing can tell when the
    /// chat pane (input, question cards, …) owns focus.
    focus_handle: gpui::FocusHandle,
    cwd: String,
    codex_program: String,
    session: Option<Arc<dyn AgentSession>>,
    starting: bool,
    /// A turn is in flight (drives the send/stop button state).
    busy: bool,
    /// When the current turn started (for the "Working (Ns)" elapsed counter).
    turn_started: Option<Instant>,
    /// Per-turn completion footers pinned to each turn's last item id:
    /// (elapsed secs, that turn's output tokens). Survives in the transcript.
    turn_done: HashMap<String, (u64, u64)>,
    /// Turns composed before the session finished connecting; flushed on Started.
    pending_send: Vec<Vec<UserInputPart>>,
    items: Vec<TimelineItem>,
    pending_approvals: Vec<ApprovalRequest>,
    /// Mid-turn questions from the agent (item/tool/requestUserInput).
    pending_user_inputs: Vec<UserInputRequest>,
    /// Selected option per (request token, question id).
    user_input_choice: HashMap<(String, String), String>,
    /// Permission escalations (item/permissions/requestApproval).
    pending_permissions: Vec<PermissionRequest>,
    /// Current turn's todo plan; replaced wholesale, cleared on turn start.
    plan: Option<(Option<String>, Vec<PlanStep>)>,
    /// Last turn error, pinned until the next turn starts.
    last_error: Option<String>,
    item_times: HashMap<String, SharedString>,
    status: SharedString,
    input: Entity<InputState>,
    /// Virtualized transcript: only visible rows are rendered/measured, so long
    /// sessions scroll smoothly instead of re-laying-out every message.
    list_state: ListState,
    /// Row blocks (message / activity-run) referencing `items` by range.
    blocks: Vec<Block>,
    /// Per-item UI version, bumped by height-changing toggles (expand / group /
    /// inline edit) so the affected row is re-measured via a minimal splice.
    item_versions: HashMap<String, u64>,
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
    /// Prior threads + whether the resume picker is open.
    threads: Vec<ThreadInfo>,
    show_threads: bool,
    /// Pane width recorded at prepaint; < 460px switches the composer to
    /// icon-only dropdowns so narrow splits don't clip the controls.
    container_width: Option<gpui::Pixels>,
    tx: Sender<ChatMsg>,
    _drain: Task<()>,
    /// 1s heartbeat that repaints the elapsed "Working (Ns)" counter while busy.
    _tick: Task<()>,
}

impl ChatView {
    pub(in crate::app) fn new(
        app: WeakEntity<crate::app::CoduxApp>,
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
        // Bottom alignment + tail follow = chat-log semantics: stick to the end
        // while streaming, stop when the user scrolls up, re-engage at bottom.
        let list_state = ListState::new(0, ListAlignment::Bottom, px(512.0));
        list_state.set_follow_mode(FollowMode::Tail);
        // Connect eagerly so the model/effort/access dropdowns and the `/` palette
        // populate from the server before the user types anything.
        spawn_session(
            tx.clone(),
            cwd.clone(),
            codex_program.clone(),
            Access::WorkspaceWrite,
            None,
            "medium".to_string(),
            None,
        );
        Self {
            app,
            focus_handle: cx.focus_handle(),
            cwd,
            codex_program,
            session: None,
            starting: true,
            busy: false,
            turn_started: None,
            turn_done: HashMap::new(),
            pending_send: Vec::new(),
            items: Vec::new(),
            pending_approvals: Vec::new(),
            pending_user_inputs: Vec::new(),
            user_input_choice: HashMap::new(),
            pending_permissions: Vec::new(),
            plan: None,
            last_error: None,
            item_times: HashMap::new(),
            status: SharedString::from("Idle"),
            input,
            list_state,
            blocks: Vec::new(),
            item_versions: HashMap::new(),
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
            threads: Vec::new(),
            show_threads: false,
            container_width: None,
            tx,
            _drain: drain,
            _tick: tick,
        }
    }

    /// Apply a burst of messages with a SINGLE snapshot + render. Streaming emits
    /// many delta events per second; cloning the timeline and re-rendering per
    /// event is what made it lag, so we coalesce a whole batch into one update.
    fn handle_batch(&mut self, batch: Vec<ChatMsg>, cx: &mut Context<Self>) {
        let mut refresh = false;
        for msg in batch {
            refresh |= self.apply_msg(msg, cx);
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
            // Tail follow (set on the list state) auto-scrolls on growth.
            self.sync_blocks();
        }
        cx.notify();
    }

    /// Apply one message to state (no snapshot / notify). Returns true if the
    /// timeline may have changed (so the batch refreshes items once at the end).
    fn apply_msg(&mut self, msg: ChatMsg, cx: &mut Context<Self>) -> bool {
        match msg {
            ChatMsg::Note(note) => {
                self.status = SharedString::from(note);
                false
            }
            ChatMsg::Started(session) => {
                self.session = Some(session);
                self.starting = false;
                self.status = SharedString::from("Ready");
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
            ChatMsg::Threads(threads) => {
                self.threads = threads;
                false
            }
            ChatMsg::Failed(err) => {
                self.starting = false;
                self.busy = false;
                self.turn_started = None;
                self.status = SharedString::from(format!("Error: {err}"));
                false
            }
            ChatMsg::Event(ev) => {
                match ev {
                    AgentEvent::ApprovalRequest(req) => self.pending_approvals.push(req),
                    AgentEvent::UserInputRequest(req) => self.pending_user_inputs.push(req),
                    AgentEvent::PermissionRequest(req) => self.pending_permissions.push(req),
                    AgentEvent::TurnPlan { explanation, steps } => {
                        self.plan = Some((explanation, steps));
                    }
                    AgentEvent::ThreadNameUpdated(name) => {
                        cx.emit(ChatViewEvent::TitleChanged(name));
                    }
                    AgentEvent::Notice { kind, message } => {
                        self.status = SharedString::from(notice_text(&kind, &message));
                    }
                    AgentEvent::TurnStarted => {
                        self.plan = None;
                        self.last_error = None;
                        self.status = SharedString::from("Generating…");
                    }
                    AgentEvent::TurnCompleted { duration_ms } => {
                        // Prefer the server's own duration and per-turn (`last`)
                        // token breakdown over locally derived numbers.
                        let secs = duration_ms
                            .map(|ms| ms.div_ceil(1000))
                            .or_else(|| self.turn_started.map(|t| t.elapsed().as_secs()))
                            .unwrap_or(0);
                        let tokens = self
                            .last_usage
                            .as_ref()
                            .map(|u| u.last_output_tokens)
                            .unwrap_or(0);
                        // Pin a permanent per-turn footer to the turn's last item.
                        if let Some(last_id) = self
                            .session
                            .as_ref()
                            .and_then(|s| s.timeline_snapshot().last().map(|it| it.id.clone()))
                        {
                            self.turn_done.insert(last_id.clone(), (secs, tokens));
                            *self.item_versions.entry(last_id).or_insert(0) += 1;
                        }
                        self.busy = false;
                        self.turn_started = None;
                        self.status = SharedString::from("Ready");
                        self.pump_queue();
                        self.update_queue_status();
                    }
                    AgentEvent::TokenUsage(u) => self.last_usage = Some(u),
                    AgentEvent::Error(err) => {
                        self.busy = false;
                        self.turn_started = None;
                        self.status = SharedString::from("Ready");
                        self.last_error = Some(err);
                        self.pump_queue();
                        self.update_queue_status();
                    }
                    AgentEvent::Status(_) => {}
                    _ => self.status = SharedString::from("Generating…"),
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
        // The user's own send always jumps to the bottom and re-arms tail follow.
        self.list_state.set_follow_mode(FollowMode::Tail);
        cx.notify();
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
        self.status = SharedString::from("Generating…");
        std::thread::spawn(move || {
            let _ = session.send_user_turn(parts);
        });
    }

    /// Reflect queue/connection state in the status line.
    fn update_queue_status(&mut self) {
        if !self.pending_send.is_empty() {
            self.status = SharedString::from(format!("Queued {}", self.pending_send.len()));
        } else if self.session.is_none() {
            self.status = SharedString::from("Connecting…");
        }
    }

    /// Fetch prior threads for this cwd (uses the live session's connection).
    fn load_threads(&mut self) {
        let Some(session) = self.session.clone() else {
            return;
        };
        let tx = self.tx.clone();
        let cwd = self.cwd.clone();
        std::thread::spawn(move || {
            let threads = session.list_threads(&cwd).unwrap_or_default();
            let _ = tx.send(ChatMsg::Threads(threads));
        });
    }

    /// Resume a prior thread in place: tear down the current session and start a
    /// resumed one whose timeline is hydrated with the thread's history.
    /// `/new`: drop the current session and start a fresh one in place — the
    /// pane keeps its split position, only the conversation resets.
    fn start_new_session(&mut self, cx: &mut Context<Self>) {
        if let Some(old) = self.session.take() {
            std::thread::spawn(move || old.shutdown());
        }
        self.items.clear();
        self.item_times.clear();
        self.item_versions.clear();
        self.pending_approvals.clear();
        self.pending_user_inputs.clear();
        self.user_input_choice.clear();
        self.pending_permissions.clear();
        self.plan = None;
        self.last_error = None;
        self.turn_done.clear();
        self.sync_blocks();
        self.busy = false;
        self.starting = true;
        self.status = SharedString::from("Connecting…");
        spawn_session(
            self.tx.clone(),
            self.cwd.clone(),
            self.codex_program.clone(),
            self.access,
            self.model.clone(),
            self.effort.to_string(),
            None,
        );
        cx.notify();
    }

    fn do_resume(&mut self, thread_id: String, cx: &mut Context<Self>) {
        self.show_threads = false;
        if let Some(old) = self.session.take() {
            std::thread::spawn(move || old.shutdown());
        }
        self.items.clear();
        self.item_times.clear();
        self.item_versions.clear();
        self.pending_approvals.clear();
        self.pending_user_inputs.clear();
        self.user_input_choice.clear();
        self.pending_permissions.clear();
        self.plan = None;
        self.last_error = None;
        self.turn_done.clear();
        self.sync_blocks();
        self.busy = false;
        self.starting = true;
        self.status = SharedString::from("Resuming…");
        spawn_session(
            self.tx.clone(),
            self.cwd.clone(),
            self.codex_program.clone(),
            self.access,
            self.model.clone(),
            self.effort.to_string(),
            Some(thread_id),
        );
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
        self.turn_started = None;
        self.status = SharedString::from("Interrupted");
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
        self.editing = Some((id.clone(), input));
        self.bump_item(&id);
        cx.notify();
    }

    fn cancel_edit(&mut self, cx: &mut Context<Self>) {
        if let Some((id, _)) = self.editing.take() {
            self.bump_item(&id);
        }
        cx.notify();
    }

    fn toggle_expand(&mut self, id: String, cx: &mut Context<Self>) {
        if !self.expanded.remove(&id) {
            self.expanded.insert(id.clone());
        }
        // Per-file diff keys look like "<item id>::<path>"; the row is the item.
        let base = id.split("::").next().unwrap_or(&id).to_string();
        self.bump_item(&base);
        cx.notify();
    }

    fn toggle_group(&mut self, key: String, currently_open: bool, cx: &mut Context<Self>) {
        self.group_open.insert(key.clone(), !currently_open);
        self.bump_item(&key);
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
        self.sync_blocks();
        self.busy = true;
        self.status = SharedString::from("Resending…");
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
        self.status = SharedString::from("Connecting…");
        spawn_session(
            self.tx.clone(),
            self.cwd.clone(),
            self.codex_program.clone(),
            self.access,
            self.model.clone(),
            self.effort.to_string(),
            None,
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
                self.start_new_session(cx);
                return;
            }
            "/status" => {
                self.status = self.usage_summary();
                cx.notify();
                return;
            }
            "/resume" => {
                self.show_threads = true;
                self.load_threads();
                cx.notify();
                return;
            }
            _ => {}
        }

        let Some(session) = self.session.clone() else {
            self.status = SharedString::from("Connecting to Codex…");
            self.ensure_session();
            cx.notify();
            return;
        };

        match cmd {
            "/interrupt" => {
                std::thread::spawn(move || {
                    let _ = session.interrupt();
                });
                self.status = SharedString::from("Interrupting…");
            }
            "/compact" => {
                std::thread::spawn(move || {
                    let _ = session.compact();
                });
                self.status = SharedString::from("Compacting…");
            }
            "/review" => {
                std::thread::spawn(move || {
                    let _ = session.review_uncommitted();
                });
                self.status = SharedString::from("Reviewing…");
            }
            "/model" => {
                if arg.is_empty() {
                    self.set_input("/model ", window, cx);
                    self.status = SharedString::from("用法: /model <名称>");
                } else {
                    session.set_model(Some(arg.to_string()));
                    self.model = Some(arg.to_string());
                    self.status = SharedString::from(format!("Model: {arg}"));
                }
            }
            "/effort" => {
                if arg.is_empty() {
                    self.set_input("/effort ", window, cx);
                    self.status = SharedString::from("用法: /effort <low|medium|high|xhigh>");
                } else {
                    session.set_effort(Some(arg.to_string()));
                    self.effort = SharedString::from(arg.to_string());
                    self.status = SharedString::from(format!("Effort: {arg}"));
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
                TimelineKind::Reasoning => "Thinking",
                TimelineKind::Command => "Running command",
                TimelineKind::FileChange => "Editing files",
                TimelineKind::ToolCall => "Calling tool",
                TimelineKind::AssistantMessage if it.status == ItemStatus::InProgress => "Writing",
                _ => "Working",
            },
            None => "Working",
        }
    }

    fn usage_summary(&self) -> SharedString {
        match &self.last_usage {
            Some(u) => SharedString::from(format!(
                "Usage · total {} (input {} / output {} / cached {}) · last turn ↓ {}",
                u.total_tokens,
                u.input_tokens,
                u.output_tokens,
                u.cached_input_tokens,
                u.last_output_tokens
            )),
            None => SharedString::from("No token usage yet"),
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

    /// Approve a command and persist its proposed execpolicy amendment.
    fn respond_amendment(&mut self, token: String, amendment: Vec<String>, cx: &mut Context<Self>) {
        if let Some(session) = &self.session {
            let _ = session.respond_approval_amendment(&token, amendment);
        }
        self.pending_approvals.retain(|req| req.token != token);
        cx.notify();
    }

    /// Answer a permission escalation: grant `granted` for `scope` (`{}` = deny).
    fn respond_permission(
        &mut self,
        token: String,
        granted: serde_json::Value,
        scope: &'static str,
        cx: &mut Context<Self>,
    ) {
        if let Some(session) = &self.session {
            let _ = session.respond_permissions(&token, granted, scope);
        }
        self.pending_permissions.retain(|req| req.token != token);
        cx.notify();
    }

    fn choose_user_input(
        &mut self,
        token: String,
        qid: String,
        value: String,
        cx: &mut Context<Self>,
    ) {
        self.user_input_choice.insert((token, qid), value);
        cx.notify();
    }

    /// Submit a question card once every question has an answer. `others` maps
    /// question id → free-text input (the "other"/secret answers).
    fn submit_user_input(
        &mut self,
        token: String,
        others: Vec<(String, Entity<InputState>)>,
        cx: &mut Context<Self>,
    ) {
        let Some(pos) = self
            .pending_user_inputs
            .iter()
            .position(|req| req.token == token)
        else {
            return;
        };
        let req = &self.pending_user_inputs[pos];
        let mut answers: Vec<(String, Vec<String>)> = Vec::new();
        for q in &req.questions {
            let choice = self
                .user_input_choice
                .get(&(token.clone(), q.id.clone()))
                .cloned();
            let other_text = others
                .iter()
                .find(|(qid, _)| qid == &q.id)
                .map(|(_, input)| input.read(cx).value().trim().to_string())
                .unwrap_or_default();
            let answer = match choice {
                Some(c) if !c.is_empty() => c,
                _ => other_text,
            };
            if answer.is_empty() && !q.options.is_empty() {
                self.status = SharedString::from("请先回答所有问题");
                cx.notify();
                return;
            }
            answers.push((q.id.clone(), vec![answer]));
        }
        if let Some(session) = &self.session {
            let _ = session.respond_user_input(&token, answers);
        }
        self.pending_user_inputs.remove(pos);
        self.user_input_choice.retain(|(t, _), _| t != &token);
        cx.notify();
    }

    fn remove_queued(&mut self, idx: usize, cx: &mut Context<Self>) {
        if idx < self.pending_send.len() {
            self.pending_send.remove(idx);
            self.update_queue_status();
            cx.notify();
        }
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

    /// Whether any part of this chat pane (composer, cards) owns window focus.
    pub(in crate::app) fn contains_focused(&self, window: &Window, cx: &gpui::App) -> bool {
        self.focus_handle.contains_focused(window, cx)
    }

    /// 正文字号实时读终端字号设置，与终端一致；其余层级固定。
    fn body_font(&self, cx: &gpui::App) -> gpui::Pixels {
        self.app
            .upgrade()
            .map(|app| {
                px(app
                    .read(cx)
                    .state
                    .settings
                    .terminal_font_size
                    .parse::<f32>()
                    .unwrap_or(14.0)
                    .clamp(8.0, 28.0))
            })
            .unwrap_or_else(|| px(14.0))
    }

    /// Palette + type ramp. Colors come from the theme; sizes derive from the
    /// terminal font size: body N, titles/menus N-1, secondary N-2, stamps N-3.
    fn pal(&self, cx: &mut Context<Self>) -> Pal {
        let body = self.body_font(cx);
        let floor = |v: gpui::Pixels| if v < px(10.0) { px(10.0) } else { v };
        let theme = cx.theme();
        Pal {
            fg: theme.foreground,
            muted: theme.muted_foreground,
            border: theme.border,
            secondary: theme.secondary,
            primary: theme.primary,
            primary_fg: theme.primary_foreground,
            danger: theme.danger,
            bubble: theme.secondary,
            body,
            md: floor(body - px(1.0)),
            sm: floor(body - px(2.0)),
            xs: floor(body - px(3.0)),
        }
    }

    /// Bump an item's UI version (its row height changed) and reconcile.
    fn bump_item(&mut self, id: &str) {
        *self.item_versions.entry(id.to_string()).or_insert(0) += 1;
        self.sync_blocks();
    }

    /// Rebuild the block list from `items` and reconcile the virtualized list
    /// with a minimal splice: streaming appends re-measure only the changed
    /// tail, and a reader scrolled up in history keeps their position.
    fn sync_blocks(&mut self) {
        let mut blocks: Vec<Block> = Vec::new();
        let mut group_start: Option<usize> = None;
        for (ix, it) in self.items.iter().enumerate() {
            if it.kind == TimelineKind::Reasoning && it.text.is_empty() {
                continue; // placeholders are filtered at render too
            }
            // 文件改动独立成卡（codex app 风格），不并入活动组。
            let is_msg = matches!(
                it.kind,
                TimelineKind::UserPrompt | TimelineKind::AssistantMessage | TimelineKind::FileChange
            );
            if is_msg {
                if let Some(start) = group_start.take() {
                    blocks.push(self.make_block(start..ix));
                }
                blocks.push(self.make_block(ix..ix + 1));
            } else if group_start.is_none() {
                group_start = Some(ix);
            }
        }
        if let Some(start) = group_start {
            blocks.push(self.make_block(start..self.items.len()));
        }

        let old = &self.blocks;
        if *old == blocks {
            return;
        }
        let mut prefix = 0;
        while prefix < old.len() && prefix < blocks.len() && old[prefix] == blocks[prefix] {
            prefix += 1;
        }
        let mut suffix = 0;
        while suffix < old.len() - prefix
            && suffix < blocks.len() - prefix
            && old[old.len() - 1 - suffix] == blocks[blocks.len() - 1 - suffix]
        {
            suffix += 1;
        }
        self.list_state
            .splice(prefix..old.len() - suffix, blocks.len() - suffix - prefix);
        self.blocks = blocks;
    }

    fn make_block(&self, range: std::ops::Range<usize>) -> Block {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for it in &self.items[range.clone()] {
            it.id.hash(&mut h);
            it.text.len().hash(&mut h);
            it.output.len().hash(&mut h);
            match it.status {
                ItemStatus::InProgress => 0u8,
                ItemStatus::Completed => 1u8,
                ItemStatus::Failed => 2u8,
            }
            .hash(&mut h);
            self.item_versions.get(&it.id).copied().unwrap_or(0).hash(&mut h);
        }
        Block {
            range,
            fingerprint: h.finish(),
        }
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
    /// Type ramp derived from the terminal font size (see [`ChatView::pal`]).
    body: gpui::Pixels,
    md: gpui::Pixels,
    sm: gpui::Pixels,
    xs: gpui::Pixels,
}

impl Render for ChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pal = self.pal(cx);
        let theme = cx.theme();
        let input_bg = theme.background;
        let mono = theme.mono_font_family.clone();
        let input_value = self.input.read(cx).value().to_string();
        let compact = self.container_width.is_some_and(|w| w < px(460.0));

        let approvals: Vec<Div> = self
            .pending_approvals
            .iter()
            .map(|req| self.render_approval(req, pal, mono.clone(), cx))
            .collect();
        let user_inputs: Vec<Div> = self
            .pending_user_inputs
            .iter()
            .map(|req| self.render_user_input(req, pal, window, cx))
            .collect();
        let permissions: Vec<Div> = self
            .pending_permissions
            .iter()
            .map(|req| self.render_permission(req, pal, cx))
            .collect();
        let plan_card = self
            .plan
            .as_ref()
            .filter(|(_, steps)| !steps.is_empty())
            .map(|(explanation, steps)| render_plan(explanation.as_deref(), steps, pal));
        let error_card = self
            .last_error
            .clone()
            .map(|message| self.render_error(message, pal, cx));
        let has_rows = !self.blocks.is_empty();
        let has_pinned = !approvals.is_empty()
            || !user_inputs.is_empty()
            || !permissions.is_empty()
            || plan_card.is_some()
            || error_card.is_some()
            || self.show_working();

        div()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .track_focus(&self.focus_handle)
            .on_prepaint({
                let view = cx.entity();
                move |bounds, _, cx| {
                    view.update(cx, |view, cx| {
                        let width = bounds.size.width;
                        if view
                            .container_width
                            .is_none_or(|recorded| (recorded - width).abs() > px(1.0))
                        {
                            view.container_width = Some(width);
                            cx.notify();
                        }
                    });
                }
            })
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .when(!has_rows, |this| this.child(render_empty_state(pal)))
                    // Virtualized transcript: only visible rows render/measure,
                    // so long sessions scroll smoothly.
                    .when(has_rows, |this| {
                        this.child(
                            list(
                                self.list_state.clone(),
                                cx.processor(|view, ix: usize, window, cx| {
                                    view.render_block(ix, window, cx)
                                }),
                            )
                            .size_full(),
                        )
                    }),
            )
            // Pinned strip: plan / approvals / questions / queue / errors and the
            // working/done footer stay visible above the composer.
            .when(has_pinned, |this| {
                this.child(
                    div()
                        .flex_none()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .px(px(12.0))
                        .pt(px(8.0))
                        .when_some(plan_card, |this, card| this.child(card))
                        .children(approvals)
                        .children(user_inputs)
                        .children(permissions)
                        .when_some(error_card, |this, card| this.child(card))
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
                            let tokens = self
                                .last_usage
                                .as_ref()
                                .map(|u| u.last_output_tokens)
                                .unwrap_or(0);
                            let meta = if tokens > 0 {
                                format!("{elapsed} · ↓ {}", fmt_tokens(tokens))
                            } else {
                                elapsed.clone()
                            };
                            this.child(render_working(pal, self.working_word(), meta, secs % 2 == 0))
                        }),
                )
            })
            .child(self.render_composer(pal, input_bg, &input_value, compact, cx))
    }
}

impl ChatView {
    /// Render one virtualized transcript row (called by the list for visible
    /// rows only). `blocks[ix]` names an `items` range; a single item renders
    /// as its message/card, a run renders as a collapsible activity group.
    fn render_block(
        &mut self,
        ix: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(block) = self.blocks.get(ix).cloned() else {
            return div().into_any_element();
        };
        let pal = self.pal(cx);
        let mono = cx.theme().mono_font_family.clone();
        let refs: Vec<&TimelineItem> = self.items[block.range]
            .iter()
            .filter(|it| !(it.kind == TimelineKind::Reasoning && it.text.is_empty()))
            .collect();
        let row = match refs.as_slice() {
            [] => div(),
            [single] => self.render_row(single, pal, mono, cx),
            _ => self.render_activity_block(&refs, pal, mono, cx),
        };
        // A turn that ended on an item in this block keeps its footer forever.
        let done_meta = refs
            .iter()
            .rev()
            .find_map(|it| self.turn_done.get(&it.id).copied());
        div()
            .flex()
            .flex_col()
            .gap_1()
            .px(px(12.0))
            .pt(if ix == 0 { px(12.0) } else { px(0.0) })
            .pb(px(12.0))
            .child(row)
            .when_some(done_meta, |this, (secs, tokens)| {
                this.child(render_done(pal, secs, tokens))
            })
            .into_any_element()
    }

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
                            .bg(pal.primary.opacity(0.1))
                            .border_1()
                            .border_color(pal.primary.opacity(0.18))
                            .text_size(pal.body)
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
                                    .text_size(pal.xs)
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
                    .text_size(pal.body)
                    .text_color(pal.fg)
                    .child(
                        TextView::markdown(
                            SharedString::from(format!("md-{}", item.id)),
                            text,
                        )
                        .selectable(true),
                    ),
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

    /// Approval card with the full decision set (codex app style): 允许 / 本会话
    /// 总是允许 / 允许此类命令 (execpolicy amendment) / 拒绝 / 拒绝并中断.
    fn render_approval(
        &self,
        req: &ApprovalRequest,
        pal: Pal,
        mono: SharedString,
        cx: &mut Context<Self>,
    ) -> Div {
        let is_cmd = req.method.contains("commandExecution") || req.method == "execCommandApproval";
        let is_elicit = req.method.contains("elicitation");
        let title = if is_cmd {
            "需要批准 · 执行命令"
        } else if req.method.contains("fileChange") || req.method == "applyPatchApproval" {
            "需要批准 · 应用文件改动"
        } else if is_elicit {
            "需要批准 · MCP 请求"
        } else {
            "需要批准"
        };
        let cwd = req
            .raw
            .get("cwd")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let amendment: Option<Vec<String>> = req
            .raw
            .get("proposedExecpolicyAmendment")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|s| s.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .filter(|v| !v.is_empty());
        let decision_button = |label: &'static str,
                               decision: ApprovalDecision,
                               danger: bool,
                               cx: &mut Context<Self>| {
            let token = req.token.clone();
            let button = Button::new(SharedString::from(format!(
                "appr-{label}-{}",
                req.token
            )))
            .ghost()
            .with_size(Size::Small)
            .child(label)
            .on_click(cx.listener(move |view, _e, _w, cx| {
                view.respond_approval(token.clone(), decision, cx)
            }));
            if danger { button.text_color(pal.danger) } else { button }
        };
        div()
            .flex()
            .flex_col()
            .gap_2()
            .rounded(px(10.0))
            .p(px(10.0))
            .border_1()
            .border_color(pal.primary.opacity(0.4))
            .bg(pal.primary.opacity(0.06))
            .child(
                div()
                    .text_size(pal.sm)
                    .text_color(pal.primary)
                    .child(title),
            )
            .child(
                div()
                    .text_size(if is_cmd { pal.sm } else { pal.md })
                    .text_color(pal.fg)
                    .when(is_cmd, |s| s.font_family(mono))
                    .child(req.summary.clone()),
            )
            .when_some(cwd, |this, cwd| {
                this.child(div().text_size(pal.xs).text_color(pal.muted).child(cwd))
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child({
                        let token = req.token.clone();
                        Button::new(SharedString::from(format!("appr-ok-{}", req.token)))
                            .primary()
                            .with_size(Size::Small)
                            .child("允许")
                            .on_click(cx.listener(move |view, _e, _w, cx| {
                                view.respond_approval(
                                    token.clone(),
                                    ApprovalDecision::Accept,
                                    cx,
                                )
                            }))
                    })
                    .when(!is_elicit, |this| {
                        this.child(decision_button(
                            "本会话总是允许",
                            ApprovalDecision::AcceptForSession,
                            false,
                            cx,
                        ))
                    })
                    .when_some(amendment, |this, amendment| {
                        let token = req.token.clone();
                        this.child(
                            Button::new(SharedString::from(format!("appr-amend-{}", req.token)))
                                .ghost()
                                .with_size(Size::Small)
                                .child("总是允许此类命令")
                                .on_click(cx.listener(move |view, _e, _w, cx| {
                                    view.respond_amendment(token.clone(), amendment.clone(), cx)
                                })),
                        )
                    })
                    .child(decision_button("拒绝", ApprovalDecision::Decline, true, cx))
                    .child(decision_button(
                        "拒绝并中断",
                        ApprovalDecision::Cancel,
                        true,
                        cx,
                    )),
            )
    }

    /// Mid-turn question card (item/tool/requestUserInput): options are radio
    /// rows, `isOther`/optionless questions get a free-text (masked if secret)
    /// input, one submit per request.
    fn render_user_input(
        &self,
        req: &UserInputRequest,
        pal: Pal,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let token = req.token.clone();
        let mut others: Vec<(String, Entity<InputState>)> = Vec::new();
        let mut card = div()
            .flex()
            .flex_col()
            .gap_2()
            .rounded(px(10.0))
            .p(px(10.0))
            .border_1()
            .border_color(pal.primary.opacity(0.4))
            .bg(pal.primary.opacity(0.06))
            .child(
                div()
                    .text_size(pal.sm)
                    .text_color(pal.primary)
                    .child("需要你的回答"),
            );
        for q in &req.questions {
            if !q.header.is_empty() {
                card = card.child(
                    div()
                        .text_size(pal.xs)
                        .text_color(pal.muted)
                        .child(q.header.clone()),
                );
            }
            card = card.child(
                div()
                    .text_size(pal.md)
                    .text_color(pal.fg)
                    .child(q.question.clone()),
            );
            let selected = self
                .user_input_choice
                .get(&(token.clone(), q.id.clone()))
                .cloned();
            for opt in &q.options {
                let is_selected = selected.as_deref() == Some(opt.label.as_str());
                let token2 = token.clone();
                let qid = q.id.clone();
                let value = opt.label.clone();
                card = card.child(
                    div()
                        .id(SharedString::from(format!(
                            "uiq-{}-{}-{}",
                            req.token, q.id, opt.label
                        )))
                        .flex()
                        .items_center()
                        .gap_2()
                        .rounded(px(6.0))
                        .px(px(8.0))
                        .py(px(5.0))
                        .cursor_pointer()
                        .when(is_selected, |s| s.bg(pal.primary.opacity(0.12)))
                        .hover(|s| s.bg(pal.bubble))
                        .on_click(cx.listener(move |view, _e, _w, cx| {
                            view.choose_user_input(
                                token2.clone(),
                                qid.clone(),
                                value.clone(),
                                cx,
                            )
                        }))
                        .child(
                            Icon::new(if is_selected {
                                HeroIconName::CheckCircle
                            } else {
                                HeroIconName::MinusCircle
                            })
                            .size_3()
                            .text_color(if is_selected { pal.primary } else { pal.muted }),
                        )
                        .child(
                            div()
                                .flex_none()
                                .text_size(pal.md)
                                .text_color(pal.fg)
                                .child(opt.label.clone()),
                        )
                        .when(!opt.description.is_empty(), |s| {
                            s.child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(pal.sm)
                                    .text_color(pal.muted)
                                    .child(opt.description.clone()),
                            )
                        }),
                );
            }
            if q.is_other || q.options.is_empty() {
                let is_secret = q.is_secret;
                let input = window.use_keyed_state(
                    SharedString::from(format!("chat-uinput-{}-{}", req.token, q.id)),
                    cx,
                    move |window, cx| {
                        InputState::new(window, cx)
                            .placeholder("输入回答…")
                            .masked(is_secret)
                    },
                );
                others.push((q.id.clone(), input.clone()));
                card = card.child(
                    div()
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(pal.border)
                        .p(px(4.0))
                        .child(Input::new(&input).appearance(false)),
                );
            }
        }
        let submit_token = token.clone();
        card.child(
            div().flex().justify_end().child(
                Button::new(SharedString::from(format!("uiq-submit-{}", req.token)))
                    .primary()
                    .with_size(Size::Small)
                    .child("提交回答")
                    .on_click(cx.listener(move |view, _e, _w, cx| {
                        view.submit_user_input(submit_token.clone(), others.clone(), cx)
                    })),
            ),
        )
    }

    /// Permission-escalation card: grant the requested profile for this turn or
    /// the whole session, or deny (grant nothing).
    fn render_permission(&self, req: &PermissionRequest, pal: Pal, cx: &mut Context<Self>) -> Div {
        let summary = permission_summary(&req.requested);
        let grant_turn = (req.token.clone(), req.requested.clone());
        let grant_session = (req.token.clone(), req.requested.clone());
        let deny_token = req.token.clone();
        div()
            .flex()
            .flex_col()
            .gap_2()
            .rounded(px(10.0))
            .p(px(10.0))
            .border_1()
            .border_color(pal.primary.opacity(0.4))
            .bg(pal.primary.opacity(0.06))
            .child(
                div()
                    .text_size(pal.sm)
                    .text_color(pal.primary)
                    .child("需要批准 · 扩展权限"),
            )
            .when_some(req.reason.clone(), |this, reason| {
                this.child(div().text_size(pal.md).text_color(pal.fg).child(reason))
            })
            .child(div().text_size(pal.sm).text_color(pal.muted).child(summary))
            .when(!req.cwd.is_empty(), |this| {
                this.child(
                    div()
                        .text_size(pal.xs)
                        .text_color(pal.muted)
                        .child(req.cwd.clone()),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        Button::new(SharedString::from(format!("perm-turn-{}", req.token)))
                            .primary()
                            .with_size(Size::Small)
                            .child("本回合允许")
                            .on_click(cx.listener(move |view, _e, _w, cx| {
                                view.respond_permission(
                                    grant_turn.0.clone(),
                                    grant_turn.1.clone(),
                                    "turn",
                                    cx,
                                )
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("perm-session-{}", req.token)))
                            .ghost()
                            .with_size(Size::Small)
                            .child("整个会话允许")
                            .on_click(cx.listener(move |view, _e, _w, cx| {
                                view.respond_permission(
                                    grant_session.0.clone(),
                                    grant_session.1.clone(),
                                    "session",
                                    cx,
                                )
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("perm-deny-{}", req.token)))
                            .ghost()
                            .with_size(Size::Small)
                            .text_color(pal.danger)
                            .child("拒绝")
                            .on_click(cx.listener(move |view, _e, _w, cx| {
                                view.respond_permission(
                                    deny_token.clone(),
                                    serde_json::json!({}),
                                    "turn",
                                    cx,
                                )
                            })),
                    ),
            )
    }

    /// Pinned red card for a turn error (the status line alone is too transient).
    fn render_error(&self, message: String, pal: Pal, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(10.0))
            .p(px(8.0))
            .border_1()
            .border_color(pal.danger.opacity(0.4))
            .bg(pal.danger.opacity(0.06))
            .child(
                Icon::new(HeroIconName::ExclamationTriangle)
                    .size_3()
                    .text_color(pal.danger),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(pal.sm)
                    .text_color(pal.danger)
                    .child(message),
            )
            .child(icon_action(
                SharedString::from("err-dismiss"),
                HeroIconName::XMark,
                pal,
                cx.listener(|view, _e, _w, cx| {
                    view.last_error = None;
                    cx.notify();
                }),
            ))
    }

    /// One queued (not yet sent) turn inside the composer-top dock.
    fn render_queued(
        &self,
        idx: usize,
        parts: &[UserInputPart],
        pal: Pal,
        cx: &mut Context<Self>,
    ) -> Div {
        div()
            .flex()
            .items_center()
            .gap_2()
            .px(px(10.0))
            .py(px(6.0))
            .child(Icon::new(HeroIconName::Clock).size_3().text_color(pal.muted))
            .child(div().text_size(pal.xs).text_color(pal.muted).child("Queued"))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(pal.sm)
                    .text_color(pal.fg)
                    .child(queued_label(parts)),
            )
            .child(icon_action(
                SharedString::from(format!("queued-x-{idx}")),
                HeroIconName::XMark,
                pal,
                cx.listener(move |view, _e, _w, cx| view.remove_queued(idx, cx)),
            ))
    }

    fn render_composer(
        &self,
        pal: Pal,
        input_bg: gpui::Hsla,
        input_value: &str,
        compact: bool,
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

        // Queued turns dock onto the composer top (codex app style): slightly
        // narrower, top-rounded only, visually attached to the box below.
        let queued_dock = (!self.pending_send.is_empty()).then(|| {
            let mut dock = div()
                .mx(px(12.0))
                .rounded_tl(px(10.0))
                .rounded_tr(px(10.0))
                .border_t_1()
                .border_l_1()
                .border_r_1()
                .border_color(pal.border)
                .bg(pal.secondary.opacity(0.5))
                .flex()
                .flex_col();
            for (idx, parts) in self.pending_send.iter().enumerate() {
                dock = dock.child(self.render_queued(idx, parts, pal, cx));
            }
            dock
        });

        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap_2()
            .p(px(10.0))
            .when(show_slash, |this| this.child(self.render_slash_menu(input_value, pal, cx)))
            .when(show_mention, |this| this.child(self.render_mention_menu(pal, cx)))
            .when(self.show_threads, |this| this.child(self.render_threads_menu(pal, cx)))
            .child(
                div().flex().flex_col().children(queued_dock).child(
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
                                    .child(self.access_button(access, access_color, pal, compact, cx)),
                            )
                            // Right cluster: model, effort, send.
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(self.model_button(model_label, pal, compact, cx))
                                    .child(self.effort_button(effort_label, pal, compact, cx))
                                    .child(self.send_button(pal, cx)),
                            ),
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
                    .text_size(pal.sm)
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
                    .text_size(pal.sm)
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

    /// Resume picker: prior threads for this cwd (preview + relative time).
    fn render_threads_menu(&self, pal: Pal, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Local::now().timestamp();
        let mut menu = div()
            .id("agent-threads-menu")
            .flex()
            .flex_col()
            .max_h(px(320.0))
            .overflow_y_scroll()
            .rounded(px(10.0))
            .border_1()
            .border_color(pal.border)
            .bg(pal.secondary)
            .p(px(4.0));
        // Header with a close affordance.
        menu = menu.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .px(px(8.0))
                .py(px(4.0))
                .child(
                    div()
                        .text_size(pal.xs)
                        .text_color(pal.muted)
                        .child("历史会话"),
                )
                .child(
                    div()
                        .id("threads-close")
                        .p(px(2.0))
                        .rounded(px(4.0))
                        .text_color(pal.muted)
                        .cursor_pointer()
                        .hover(|s| s.bg(pal.bubble))
                        .on_click(cx.listener(|view, _e, _w, cx| {
                            view.show_threads = false;
                            cx.notify();
                        }))
                        .child(Icon::new(HeroIconName::XMark).size_3()),
                ),
        );
        if self.threads.is_empty() {
            return menu.child(
                div()
                    .p(px(8.0))
                    .text_size(pal.sm)
                    .text_color(pal.muted)
                    .child("没有历史会话"),
            );
        }
        for t in &self.threads {
            let id = t.id.clone();
            let when = relative_time(now, t.updated_at);
            menu = menu.child(
                div()
                    .id(SharedString::from(format!("thread-{}", t.id)))
                    .flex()
                    .items_center()
                    .gap_2()
                    .rounded(px(6.0))
                    .px(px(8.0))
                    .py(px(6.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(pal.bubble))
                    .on_click(cx.listener(move |view, _e, _w, cx| {
                        view.do_resume(id.clone(), cx)
                    }))
                    .child(Icon::new(HeroIconName::Clock).size_3().text_color(pal.muted))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(pal.md)
                            .text_color(pal.fg)
                            .child(t.preview.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(pal.xs)
                            .text_color(pal.muted)
                            .child(when),
                    ),
            );
        }
        menu
    }

    fn access_button(
        &self,
        access: Access,
        color: gpui::Hsla,
        pal: Pal,
        compact: bool,
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
                    .when(!compact, |s| {
                        s.child(div().text_size(pal.sm).child(access.label()))
                    })
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
        compact: bool,
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
                    .when(!compact, |s| s.child(div().text_size(pal.sm).child(label)))
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
        compact: bool,
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
                    .when(!compact, |s| s.child(div().text_size(pal.sm).child(label)))
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
                    .text_size(pal.sm)
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
                    .text_size(pal.xs)
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
        .child(div().flex_none().text_size(pal.md).text_color(pal.fg).child(name))
        .when(!desc.is_empty(), |this| {
            // flex_1 + truncate:中文没有词边界,任由布局收缩会塌成一列单字。
            this.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(pal.sm)
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

/// Centered placeholder shown before the first message.
fn render_empty_state(pal: Pal) -> Div {
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_3()
        .child(
            div()
                .size(px(44.0))
                .rounded(px(12.0))
                .bg(pal.primary.opacity(0.1))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Icon::new(HeroIconName::Sparkles)
                        .size_4()
                        .text_color(pal.primary),
                ),
        )
        .child(
            div()
                .text_size(pal.md)
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(pal.fg)
                .child("开始与 Codex 对话"),
        )
        .child(
            div()
                .text_size(pal.sm)
                .text_color(pal.muted)
                .child("输入 / 调出命令 · @ 引用文件 · Shift+Enter 换行"),
        )
}

/// Working indicator shown for the whole turn: theme-color verb + elapsed time /
/// token meta (codex CLI style, e.g. "执行命令… 已执行 2m 18s · ↓ 5.8k"). Updated
/// once a second by the heartbeat — intentionally NOT a 60fps animation, which
/// would force the whole transcript to re-render every frame.
fn render_working(pal: Pal, word: &str, meta: String, pulse: bool) -> Div {
    div().flex().w_full().child(
        div()
            .flex()
            .items_center()
            .gap_2()
            .rounded_full()
            .px(px(10.0))
            .py(px(4.0))
            .bg(pal.primary.opacity(0.08))
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
                    .text_size(pal.md)
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(pal.primary)
                    .child(format!("{word}…")),
            )
            .child(div().text_size(pal.sm).text_color(pal.muted).child(meta)),
    )
}

/// "完成" footer after a turn: a check + elapsed time + output tokens. Derived
/// from the turn's metrics — not a stored/fabricated message.
fn render_done(pal: Pal, secs: u64, tokens: u64) -> Div {
    let elapsed = if secs >= 60 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    };
    let meta = if tokens > 0 {
        format!("Done · {elapsed} · ↓ {}", fmt_tokens(tokens))
    } else {
        format!("Done · {elapsed}")
    };
    div()
        .flex()
        .w_full()
        .items_center()
        .gap_2()
        .py(px(2.0))
        .child(Icon::new(HeroIconName::Check).size_3().text_color(pal.muted))
        .child(div().text_size(pal.sm).text_color(pal.muted).child(meta))
}

/// The turn's todo plan (turn/plan/updated), pinned while the turn runs.
fn render_plan(explanation: Option<&str>, steps: &[PlanStep], pal: Pal) -> Div {
    let done = steps.iter().filter(|s| s.status == "completed").count();
    let mut list = div()
        .id("chat-plan-steps")
        .flex()
        .flex_col()
        .gap_1()
        .max_h(px(160.0))
        .overflow_y_scroll();
    for (ix, step) in steps.iter().enumerate() {
        let status: AnyElement = match step.status.as_str() {
            "completed" => Icon::new(HeroIconName::Check)
                .size_3()
                .text_color(pal.muted)
                .into_any_element(),
            "inProgress" => div()
                .size(px(7.0))
                .rounded_full()
                .bg(pal.primary)
                .into_any_element(),
            _ => div()
                .size(px(7.0))
                .rounded_full()
                .border_1()
                .border_color(pal.muted)
                .into_any_element(),
        };
        let color = if step.status == "completed" { pal.muted } else { pal.fg };
        list = list.child(
            div()
                .id(SharedString::from(format!("plan-step-{ix}")))
                .flex()
                .items_center()
                .gap_2()
                .child(div().flex_none().w(px(10.0)).flex().justify_center().child(status))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(pal.sm)
                        .text_color(color)
                        .child(step.step.clone()),
                ),
        );
    }
    div()
        .flex()
        .flex_col()
        .gap_1()
        .rounded(px(10.0))
        .p(px(10.0))
        .border_1()
        .border_color(pal.border)
        .bg(pal.secondary.opacity(0.35))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(Icon::new(HeroIconName::Bars3).size_3().text_color(pal.muted))
                .child(div().text_size(pal.xs).text_color(pal.muted).child("Plan"))
                .child(
                    div()
                        .text_size(pal.xs)
                        .text_color(pal.muted)
                        .child(format!("{done}/{}", steps.len())),
                ),
        )
        .when_some(explanation.filter(|e| !e.is_empty()), |this, explanation| {
            this.child(
                div()
                    .text_size(pal.sm)
                    .text_color(pal.muted)
                    .child(explanation.to_string()),
            )
        })
        .child(list)
}

/// User-facing text for advisory notices (compaction, reroute, warnings).
fn notice_text(kind: &str, message: &str) -> String {
    let label = match kind {
        "thread/compacted" => "Context compacted",
        "model/rerouted" => "Model rerouted",
        "deprecationNotice" => "Deprecation notice",
        _ => "Notice",
    };
    if message.is_empty() {
        label.to_string()
    } else {
        format!("{label}: {message}")
    }
}

/// Preview line for a queued turn: its text plus an attachment count.
fn queued_label(parts: &[UserInputPart]) -> String {
    let text = parts
        .iter()
        .find_map(|p| match p {
            UserInputPart::Text(t) => Some(t.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let extras = parts
        .iter()
        .filter(|p| !matches!(p, UserInputPart::Text(_)))
        .count();
    match (text.is_empty(), extras) {
        (false, 0) => text,
        (false, n) => format!("{text} (+{n} 附件)"),
        (true, n) => format!("{n} 个附件"),
    }
}

/// Human summary of a requested permission profile (paths / network).
fn permission_summary(requested: &serde_json::Value) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(fs) = requested.get("fileSystem").filter(|v| !v.is_null()) {
        let mut paths: Vec<String> = Vec::new();
        if let Some(entries) = fs.get("entries").and_then(|v| v.as_array()) {
            for entry in entries {
                let path = entry.get("path");
                let path = path
                    .and_then(|v| v.as_str())
                    .or_else(|| path.and_then(|v| v.get("path")).and_then(|v| v.as_str()));
                if let Some(path) = path {
                    paths.push(path.to_string());
                }
            }
        }
        for key in ["read", "write"] {
            if let Some(arr) = fs.get(key).and_then(|v| v.as_array()) {
                paths.extend(arr.iter().filter_map(|v| v.as_str().map(String::from)));
            }
        }
        if paths.is_empty() {
            parts.push("文件系统扩展访问".into());
        } else {
            parts.push(format!("文件系统: {}", paths.join(", ")));
        }
    }
    if requested
        .get("network")
        .and_then(|n| n.get("enabled"))
        .and_then(|v| v.as_bool())
        == Some(true)
    {
        parts.push("网络访问".into());
    }
    if parts.is_empty() {
        "扩展权限".into()
    } else {
        parts.join(" · ")
    }
}

fn meta_row(time: SharedString, pal: Pal) -> Div {
    div()
        .flex()
        .items_center()
        .gap_1()
        .child(div().text_size(pal.xs).text_color(pal.muted).child(time))
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
            format!("Running… {} steps", items.len())
        } else {
            format!("Ran {} steps", items.len())
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
                    .text_size(pal.sm)
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
            .w_full()
            .min_w_0()
            .rounded(px(8.0))
            .border_1()
            .border_color(pal.border)
            .bg(pal.secondary.opacity(0.35))
            .overflow_hidden()
            .child(header);
        if open {
            let mut body = div().flex().flex_col().w_full().min_w_0().px(px(4.0)).pb(px(4.0));
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
                    .text_size(pal.xs)
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
        let (label, icon) = match item.item_type.as_str() {
            "webSearch" => ("Search", HeroIconName::MagnifyingGlass),
            "mcpToolCall" => ("MCP", HeroIconName::Cog6Tooth),
            "contextCompaction" => ("Compact", HeroIconName::ArchiveBoxArrowDown),
            _ => match item.kind {
                TimelineKind::Reasoning => ("Thinking", HeroIconName::LightBulb),
                TimelineKind::Command => ("Command", HeroIconName::CommandLine),
                TimelineKind::FileChange => ("File", HeroIconName::DocumentText),
                TimelineKind::Plan => ("Plan", HeroIconName::Bars3),
                _ => ("Tool", HeroIconName::Cog6Tooth),
            },
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
        // 独立的文件改动卡默认展开(codex app 风格);expanded 集合此时反向存折叠。
        let default_open = is_filechange && !nested;
        let expanded = self.expanded.contains(&item.id) != default_open;
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
            .child(div().flex_none().text_size(pal.xs).text_color(pal.muted).child(label))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(if is_cmd { pal.sm } else { pal.md })
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
            .w_full()
            .min_w_0()
            .rounded(px(8.0))
            .overflow_hidden()
            .when(!nested, |s| {
                s.border_1()
                    .border_color(pal.border)
                    .bg(pal.secondary.opacity(0.35))
            })
            .child(header);

        if expanded && has_body {
            let mut body = div()
                .id(SharedString::from(format!("card-body-{id}")))
                .max_h(px(280.0))
                .min_w_0()
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_1()
                .px(px(8.0))
                .pb(px(8.0));
            if !body_text.is_empty() {
                body = body.child(
                    div()
                        .text_size(pal.md)
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
                                        .text_size(pal.sm)
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
                                        .child(div().text_size(pal.sm).child("打开方式"))
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
                        .text_size(pal.sm)
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

/// Coarse "刚刚 / N 分钟前 / N 小时前 / N 天前" from two unix timestamps.
fn relative_time(now: i64, then: i64) -> String {
    let d = (now - then).max(0);
    if d < 60 {
        "刚刚".into()
    } else if d < 3600 {
        format!("{} 分钟前", d / 60)
    } else if d < 86400 {
        format!("{} 小时前", d / 3600)
    } else {
        format!("{} 天前", d / 86400)
    }
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
        [] => "File changes".into(),
        [(p, _)] => format!("Edited {}", short_path(p)),
        _ => format!("Edited {} files", changes.len()),
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
        .text_size(pal.xs)
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
    let mut block = div().flex().flex_col().font_family(mono).text_size(pal.sm);
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
