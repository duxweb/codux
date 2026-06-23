//! GPUI chat view for a protocol-driven AI session (Phase 2).
//!
//! Renders the normalized, merged timeline from `codux-agent-driver` as a
//! scrollable list of cards with a message box at the bottom. The Codex session
//! runs off-thread; events arrive over a flume channel and are drained in a
//! `cx.spawn` loop (the same pattern the terminal uses), so the UI thread never
//! blocks on the app-server handshake or a turn.

use codux_agent_driver::{
    AgentEvent, ApprovalDecision, CodexAgentDriver, CodexSession, ItemStatus, SessionConfig,
    TimelineItem, TimelineKind,
};
use flume::Sender;
use gpui::{
    AppContext, Context, Div, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Task, WeakEntity, Window, div, prelude::FluentBuilder as _, px, rems,
};
use gpui_component::{
    ActiveTheme, Sizable, Size,
    button::{Button, ButtonVariants},
    input::{Input, InputState},
};

use crate::app::scroll_compat::ScrollableElement as _;
use crate::app::types::{WorkspaceSplitKind, WorkspaceView};

impl crate::app::CoduxApp {
    /// Toggle the body-split AI chat panel (Codex on the right of the terminal).
    pub(in crate::app) fn toggle_chat_split(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.workspace_split == Some(WorkspaceSplitKind::Chat) {
            self.workspace_split = None;
        } else {
            // The chat split only renders inside the Terminal view.
            self.workspace_view = WorkspaceView::Terminal;
            self.workspace_split = Some(WorkspaceSplitKind::Chat);
        }
        cx.notify();
    }
}

/// Messages from the off-thread session machinery into the view.
enum ChatMsg {
    Started(CodexSession),
    Failed(String),
    Event(AgentEvent),
}

pub(in crate::app) struct ChatView {
    cwd: String,
    codex_program: String,
    session: Option<CodexSession>,
    starting: bool,
    items: Vec<TimelineItem>,
    status: SharedString,
    input: Entity<InputState>,
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
                .placeholder("Message Codex…")
                .multi_line(true)
        });
        let drain = cx.spawn(async move |this: WeakEntity<Self>, cx| {
            while let Ok(msg) = rx.recv_async().await {
                if this.update(cx, |view, cx| view.handle_msg(msg, cx)).is_err() {
                    break; // view dropped → stop draining
                }
            }
        });
        Self {
            cwd,
            codex_program,
            session: None,
            starting: false,
            items: Vec::new(),
            status: SharedString::from("idle"),
            input,
            tx,
            _drain: drain,
        }
    }

    fn handle_msg(&mut self, msg: ChatMsg, cx: &mut Context<Self>) {
        match msg {
            ChatMsg::Started(session) => {
                self.session = Some(session);
                self.starting = false;
                self.status = SharedString::from("ready");
            }
            ChatMsg::Failed(err) => {
                self.starting = false;
                self.status = SharedString::from(format!("error: {err}"));
            }
            ChatMsg::Event(ev) => {
                // The session owns the canonical merged timeline; re-read it
                // rather than re-implementing the merge here.
                if let Some(session) = &self.session {
                    self.items = session.timeline_snapshot();
                }
                match ev {
                    AgentEvent::ApprovalRequest(req) => {
                        // Phase 2 runs read-only; auto-approve. Phase 3 adds the
                        // approval UI (accept/decline buttons on the card).
                        if let Some(session) = &self.session {
                            let _ = session.respond_approval(&req.token, ApprovalDecision::Accept);
                        }
                        self.status = SharedString::from(format!("auto-approved: {}", req.summary));
                    }
                    AgentEvent::TurnCompleted => self.status = SharedString::from("ready"),
                    AgentEvent::Error(err) => {
                        self.status = SharedString::from(format!("error: {err}"))
                    }
                    AgentEvent::Status(_) => {}
                    _ => self.status = SharedString::from("responding…"),
                }
            }
        }
        cx.notify();
    }

    fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().trim().to_string();
        if text.is_empty() {
            return;
        }
        self.input
            .update(cx, |state, cx| state.set_value("", window, cx));

        if let Some(session) = &self.session {
            // send_user_message blocks on the turn/start ack; do it off-thread.
            let session = session.clone();
            std::thread::spawn(move || {
                let _ = session.send_user_message(&text);
            });
            self.status = SharedString::from("responding…");
        } else {
            self.start_session(text);
        }
        cx.notify();
    }

    /// Spawn the app-server and send the first prompt, all off the UI thread.
    fn start_session(&mut self, first_prompt: String) {
        if self.starting {
            return;
        }
        self.starting = true;
        self.status = SharedString::from("connecting…");
        let tx = self.tx.clone();
        let cwd = self.cwd.clone();
        let program = self.codex_program.clone();
        std::thread::spawn(move || {
            let driver = CodexAgentDriver { program };
            let cfg = SessionConfig::read_only(cwd);
            let sink_tx = tx.clone();
            let sink = Box::new(move |ev: &AgentEvent| {
                let _ = sink_tx.send(ChatMsg::Event(ev.clone()));
            });
            match CodexSession::start(&driver, &cfg, sink) {
                Ok(session) => {
                    let _ = tx.send(ChatMsg::Started(session.clone()));
                    let _ = session.send_user_message(&first_prompt);
                }
                Err(err) => {
                    let _ = tx.send(ChatMsg::Failed(err));
                }
            }
        });
    }
}

impl Render for ChatView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let fg = theme.foreground;
        let muted = theme.muted_foreground;
        let border = theme.border;
        let secondary = theme.secondary;
        let primary = theme.primary;
        let mono = theme.mono_font_family.clone();

        let cards: Vec<Div> = self
            .items
            .iter()
            .map(|item| render_card(item, fg, muted, border, secondary, primary, mono.clone()))
            .collect();

        div()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            // Header: session status.
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .h(px(32.0))
                    .px(px(12.0))
                    .border_b_1()
                    .border_color(border)
                    .child(
                        div()
                            .text_size(rems(0.8))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(fg)
                            .child("Codex"),
                    )
                    .child(
                        div()
                            .text_size(rems(0.7))
                            .text_color(muted)
                            .child(self.status.clone()),
                    ),
            )
            // Body: merged timeline cards.
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .p(px(12.0))
                    .flex()
                    .flex_col()
                    .gap_2()
                    .when(cards.is_empty(), |this| {
                        this.child(
                            div()
                                .text_size(rems(0.8))
                                .text_color(muted)
                                .child("Send a message to start a Codex session."),
                        )
                    })
                    .children(cards),
            )
            // Footer: message box + send.
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_end()
                    .gap_2()
                    .p(px(12.0))
                    .border_t_1()
                    .border_color(border)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Input::new(&self.input).with_size(Size::Medium)),
                    )
                    .child(
                        Button::new("agent-chat-send")
                            .primary()
                            .child("Send")
                            .on_click(cx.listener(|view, _event, window, cx| {
                                view.submit(window, cx)
                            })),
                    ),
            )
    }
}

#[allow(clippy::too_many_arguments)]
fn render_card(
    item: &TimelineItem,
    fg: gpui::Hsla,
    muted: gpui::Hsla,
    border: gpui::Hsla,
    secondary: gpui::Hsla,
    primary: gpui::Hsla,
    mono: SharedString,
) -> Div {
    let (label, label_color, is_user) = match item.kind {
        TimelineKind::UserPrompt => ("You", primary, true),
        TimelineKind::AssistantMessage => ("Codex", fg, false),
        TimelineKind::Reasoning => ("Thinking", muted, false),
        TimelineKind::Command => ("Command", fg, false),
        TimelineKind::FileChange => ("Files", fg, false),
        TimelineKind::ToolCall => ("Tool", fg, false),
        TimelineKind::Plan => ("Plan", fg, false),
        _ => ("·", muted, false),
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
        .p(px(10.0))
        .border_1()
        .border_color(border)
        .when(is_user, |this| this.bg(secondary))
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_size(rems(0.7))
                .text_color(label_color)
                .child(format!("{label}{status_mark}"))
                .when(!item.title.is_empty() && item.kind != TimelineKind::Command, |this| {
                    this.child(div().text_color(muted).child(item.title.clone()))
                }),
        )
        // Command line (monospace) for command items.
        .when(item.kind == TimelineKind::Command, |this| {
            this.child(
                div()
                    .font_family(mono.clone())
                    .text_size(rems(0.72))
                    .text_color(fg)
                    .child(item.command.clone().unwrap_or_else(|| item.title.clone())),
            )
        })
        // Message / reasoning text.
        .when(!item.text.is_empty(), |this| {
            this.child(
                div()
                    .text_size(rems(0.8))
                    .text_color(if item.kind == TimelineKind::Reasoning { muted } else { fg })
                    .child(item.text.clone()),
            )
        })
        // Command / tool output (monospace, trimmed).
        .when(!item.output.is_empty(), |this| {
            this.child(
                div()
                    .font_family(mono)
                    .text_size(rems(0.7))
                    .text_color(muted)
                    .child(item.output.trim_end().to_string()),
            )
        })
}
