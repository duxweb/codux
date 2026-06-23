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
    AgentEvent, ApprovalDecision, ApprovalRequest, CodexAgentDriver, CodexSession, ItemStatus,
    SessionConfig, TimelineItem, TimelineKind,
};
use flume::Sender;
use gpui::{
    AppContext, ClipboardItem, Context, Div, Entity, InteractiveElement, IntoElement,
    ParentElement, Render, ScrollHandle, SharedString, StatefulInteractiveElement, Styled, Task,
    WeakEntity, Window, div, prelude::FluentBuilder as _, px, rems,
};
use gpui_component::{
    ActiveTheme, Icon, Sizable, Size,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    menu::{DropdownMenu, PopupMenuItem},
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
    vec![
        Command { icon: HeroIconName::StopCircle, name: "/interrupt", desc: "中断当前回合", token: "/interrupt" },
        Command { icon: HeroIconName::ArchiveBoxArrowDown, name: "/compact", desc: "压缩此对话的上下文", token: "/compact" },
        Command { icon: HeroIconName::CubeTransparent, name: "/model", desc: "设置模型", token: "/model " },
        Command { icon: HeroIconName::CpuChip, name: "/effort", desc: "设置推理强度 (low/medium/high/xhigh)", token: "/effort " },
    ]
}

/// Messages from the off-thread session machinery into the view.
enum ChatMsg {
    Note(String),
    Started(CodexSession),
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

pub(in crate::app) struct ChatView {
    cwd: String,
    codex_program: String,
    session: Option<CodexSession>,
    starting: bool,
    items: Vec<TimelineItem>,
    pending_approvals: Vec<ApprovalRequest>,
    item_times: HashMap<String, SharedString>,
    status: SharedString,
    input: Entity<InputState>,
    scroll: ScrollHandle,
    access: Access,
    model: Option<String>,
    effort: SharedString,
    tx: Sender<ChatMsg>,
    _drain: Task<()>,
}

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
        cx.subscribe_in(&input, window, |view, _input, event, window, cx| {
            if let InputEvent::PressEnter { shift, .. } = event
                && !*shift
            {
                view.submit(window, cx);
            }
        })
        .detach();

        let drain = cx.spawn(async move |this: WeakEntity<Self>, cx| {
            while let Ok(msg) = rx.recv_async().await {
                if this.update(cx, |view, cx| view.handle_msg(msg, cx)).is_err() {
                    break;
                }
            }
        });
        Self {
            cwd,
            codex_program,
            session: None,
            starting: false,
            items: Vec::new(),
            pending_approvals: Vec::new(),
            item_times: HashMap::new(),
            status: SharedString::from("空闲"),
            input,
            scroll: ScrollHandle::new(),
            access: Access::WorkspaceWrite,
            model: None,
            effort: SharedString::from("medium"),
            tx,
            _drain: drain,
        }
    }

    fn handle_msg(&mut self, msg: ChatMsg, cx: &mut Context<Self>) {
        match msg {
            ChatMsg::Note(note) => self.status = SharedString::from(note),
            ChatMsg::Started(session) => {
                self.session = Some(session);
                self.starting = false;
                self.status = SharedString::from("就绪");
            }
            ChatMsg::Failed(err) => {
                self.starting = false;
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
                    AgentEvent::TurnCompleted => self.status = SharedString::from("就绪"),
                    AgentEvent::Error(err) => {
                        self.status = SharedString::from(format!("错误: {err}"))
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
        if text.is_empty() {
            return;
        }
        self.input
            .update(cx, |state, cx| state.set_value("", window, cx));

        if text.starts_with('/') {
            self.run_command(&text, window, cx);
            return;
        }

        if let Some(session) = &self.session {
            let session = session.clone();
            std::thread::spawn(move || {
                let _ = session.send_user_message(&text);
            });
            self.status = SharedString::from("生成中…");
        } else {
            self.start_session(text);
        }
        cx.notify();
    }

    fn start_session(&mut self, first_prompt: String) {
        if self.starting {
            return;
        }
        self.starting = true;
        self.status = SharedString::from("连接中…");
        let tx = self.tx.clone();
        let cwd = self.cwd.clone();
        let wrapper = self.codex_program.clone();
        let access = self.access;
        let model = self.model.clone();
        let effort = self.effort.to_string();
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
                    let _ = session.send_user_message(&first_prompt);
                }
                Err(err) => {
                    let _ = tx.send(ChatMsg::Failed(err));
                }
            }
        });
    }

    fn run_command(&mut self, line: &str, window: &mut Window, cx: &mut Context<Self>) {
        let line = line.trim();
        let mut parts = line.splitn(2, char::is_whitespace);
        let cmd = parts.next().unwrap_or("");
        let arg = parts.next().unwrap_or("").trim();

        let Some(session) = self.session.clone() else {
            self.status = SharedString::from("先发一条消息以启动会话");
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
        self.model = model;
        cx.notify();
    }

    fn set_effort_value(&mut self, effort: &'static str, cx: &mut Context<Self>) {
        if let Some(session) = &self.session {
            session.set_effort(Some(effort.to_string()));
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
            danger: theme.danger,
            bubble: theme.secondary,
        };
        let input_bg = theme.background;
        let mono = theme.mono_font_family.clone();

        let input_value = self.input.read(cx).value().to_string();
        let show_menu = input_value.starts_with('/');

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
                    .children(approvals),
            )
            .child(self.render_composer(pal, input_bg, show_menu, &input_value, cx))
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
        show_menu: bool,
        input_value: &str,
        cx: &mut Context<Self>,
    ) -> Div {
        let access = self.access;
        let access_color = if access == Access::FullAccess { pal.danger } else { pal.muted };
        let model_label = self.model.clone().unwrap_or_else(|| "默认模型".into());
        let effort_label = self.effort.clone();
        let plus_entity = cx.entity();

        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap_2()
            .p(px(10.0))
            .when(show_menu, |this| this.child(self.render_slash_menu(input_value, pal, cx)))
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
                            // Left cluster: commands (+) and access mode.
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
                                            .dropdown_menu(move |mut menu, _window, _cx| {
                                                for c in commands() {
                                                    let e = plus_entity.clone();
                                                    let token = c.token;
                                                    menu = menu.item(
                                                        PopupMenuItem::new(c.name)
                                                            .icon(c.icon)
                                                            .on_click(move |_, window, cx| {
                                                                cx.update_entity(&e, |view, cx| {
                                                                    view.command_clicked(token, window, cx)
                                                                });
                                                            }),
                                                    );
                                                }
                                                menu
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

    fn access_button(
        &self,
        access: Access,
        color: gpui::Hsla,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let entity = cx.entity();
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
                let opts = [
                    (Access::ReadOnly, "只读"),
                    (Access::WorkspaceWrite, "工作区写入"),
                    (Access::FullAccess, "完全访问"),
                ];
                opts.into_iter().fold(menu, |menu, (a, label)| {
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
            .dropdown_menu(move |menu, _window, _cx| {
                let e1 = entity.clone();
                let e2 = entity.clone();
                menu.item(PopupMenuItem::new("默认模型").on_click(move |_, _w, cx| {
                    cx.update_entity(&e1, |view, cx| view.set_model_value(None, cx));
                }))
                .item(PopupMenuItem::new("自定义… (/model)").on_click(move |_, window, cx| {
                    cx.update_entity(&e2, |view, cx| view.set_input("/model ", window, cx));
                }))
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
                ["minimal", "low", "medium", "high", "xhigh"]
                    .into_iter()
                    .fold(menu, |menu, level| {
                        let e = entity.clone();
                        menu.item(PopupMenuItem::new(level).on_click(move |_, _w, cx| {
                            cx.update_entity(&e, |view, cx| view.set_effort_value(level, cx));
                        }))
                    })
            })
            .into_any_element()
    }

    fn send_button(&self, pal: Pal, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("composer-send")
            .size(px(30.0))
            .rounded_full()
            .bg(pal.primary)
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|s| s.opacity(0.85))
            .on_click(cx.listener(|view, _e, window, cx| view.submit(window, cx)))
            .child(
                Icon::new(HeroIconName::ArrowUp)
                    .size_4()
                    .text_color(pal.secondary),
            )
    }

    fn render_slash_menu(&self, filter: &str, pal: Pal, cx: &mut Context<Self>) -> Div {
        let matches: Vec<Command> = commands()
            .into_iter()
            .filter(|c| c.name.starts_with(filter.trim_end()) || filter.trim() == "/")
            .collect();
        let mut menu = div()
            .flex()
            .flex_col()
            .rounded(px(10.0))
            .border_1()
            .border_color(pal.border)
            .bg(pal.secondary)
            .p(px(4.0));
        if matches.is_empty() {
            return menu.child(
                div()
                    .p(px(8.0))
                    .text_size(rems(0.75))
                    .text_color(pal.muted)
                    .child("无匹配命令"),
            );
        }
        for c in matches {
            let token = c.token;
            menu = menu.child(
                div()
                    .id(SharedString::from(format!("slash-{}", c.name)))
                    .flex()
                    .items_center()
                    .gap_2()
                    .rounded(px(6.0))
                    .px(px(8.0))
                    .py(px(6.0))
                    .cursor_pointer()
                    .hover(|s| s.bg(pal.bubble))
                    .on_click(cx.listener(move |view, _e, window, cx| {
                        view.command_clicked(token, window, cx)
                    }))
                    .child(Icon::new(c.icon).size_4().text_color(pal.muted))
                    .child(
                        div()
                            .text_size(rems(0.8))
                            .text_color(pal.fg)
                            .child(c.name),
                    )
                    .child(
                        div()
                            .text_size(rems(0.72))
                            .text_color(pal.muted)
                            .child(c.desc),
                    ),
            );
        }
        menu
    }
}

fn relative_w() -> gpui::DefiniteLength {
    gpui::relative(0.9)
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
