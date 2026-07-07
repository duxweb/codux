use super::*;
use crate::app::ui_helpers::codux_tooltip_container;
use crate::app::workspace_shared::workspace_i18n;
use gpui::Anchor;
use gpui_component::popover::Popover;

#[derive(Clone)]
struct TerminalPaneDrag {
    pane_index: usize,
}

impl Render for TerminalPaneDrag {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size(px(1.0)).bg(cx.theme().transparent)
    }
}

#[derive(Clone, Copy, PartialEq)]
struct TerminalPaneDropPreview {
    pane_index: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TerminalSplitDivider {
    None,
    Left,
    Top,
}

pub(in crate::app) struct WorkspaceColumnView {
    toolbar_view: gpui::Entity<WorkspaceToolbarView>,
    body_view: gpui::Entity<WorkspaceBodyView>,
    assistant_view: gpui::Entity<WorkspaceAssistantView>,
}

impl WorkspaceColumnView {
    pub(in crate::app) fn new(
        toolbar_view: gpui::Entity<WorkspaceToolbarView>,
        body_view: gpui::Entity<WorkspaceBodyView>,
        assistant_view: gpui::Entity<WorkspaceAssistantView>,
    ) -> Self {
        Self {
            toolbar_view,
            body_view,
            assistant_view,
        }
    }
}

impl Render for WorkspaceColumnView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let assistant_snapshot = self.assistant_view.read(cx);
        let show_assistant = assistant_snapshot
            .snapshot
            .panel
            .is_some_and(|panel| assistant_panel_available(panel, &assistant_snapshot.snapshot));

        div()
            .flex()
            .flex_col()
            .flex_1()
            .flex_basis(px(0.0))
            .min_w_0()
            .min_h_0()
            .w_full()
            .h_full()
            .child(
                div().flex().flex_none().w_full().h(px(52.0)).child(
                    gpui::AnyView::from(self.toolbar_view.clone())
                        .cached(gpui::StyleRefinement::default().flex().w_full().h(px(52.0))),
                ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .flex_basis(px(0.0))
                    .w_full()
                    .min_w_0()
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .flex_basis(px(0.0))
                            .h_full()
                            .min_w_0()
                            .min_h_0()
                            .overflow_hidden()
                            .child(
                                gpui::AnyView::from(self.body_view.clone()).cached(
                                    gpui::StyleRefinement::default()
                                        .flex()
                                        .size_full()
                                        .min_w(px(0.0))
                                        .min_h(px(0.0)),
                                ),
                            ),
                    )
                    .when(show_assistant, |this| {
                        this.child(
                            div()
                                .flex()
                                .flex_none()
                                .flex_shrink_0()
                                .w(px(ASSISTANT_PANEL_WIDTH))
                                .min_w(px(ASSISTANT_PANEL_WIDTH))
                                .max_w(px(ASSISTANT_PANEL_WIDTH))
                                .h_full()
                                .overflow_hidden()
                                .child(
                                    gpui::AnyView::from(self.assistant_view.clone()).cached(
                                        gpui::StyleRefinement::default().flex().size_full(),
                                    ),
                                ),
                        )
                    }),
            )
    }
}
fn workspace_body_any_view<V>(view: gpui::Entity<V>) -> impl IntoElement
where
    V: Render + 'static,
{
    gpui::AnyView::from(view).cached(
        gpui::StyleRefinement::default()
            .flex()
            .flex_1()
            .flex_basis(px(0.0))
            .w_full()
            .h_full()
            .min_w(px(0.0))
            .min_h(px(0.0)),
    )
}

mod app_ext;
mod column_views;
mod stats_review_views;
mod terminal_layout;
mod terminal_view;

use column_views::{assistant_panel_available, workspace_toolbar_fingerprint};
use terminal_layout::terminal_main_split_area;
use terminal_view::TerminalPaneViewSnapshot;

pub(in crate::app) use column_views::{
    WorkspaceAssistantView, WorkspaceBodyView, WorkspaceToolbarView,
};
pub(in crate::app) use stats_review_views::{
    ReviewDiffContentView, ReviewFileListView, ReviewWorkspaceView, StatsWorkspaceView,
};
#[cfg(test)]
pub(in crate::app) use terminal_layout::{
    terminal_pane_drop_target_at_position, terminal_pane_rect,
};
pub(in crate::app) use terminal_view::{TerminalWorkspaceSnapshot, TerminalWorkspaceView};
#[derive(Clone, PartialEq)]
pub(in crate::app) struct WorkspaceToolbarSnapshot {
    fingerprint: u64,
}

#[derive(Clone, PartialEq)]
pub(in crate::app) struct WorkspaceAssistantSnapshot {
    pub(super) panel: Option<AssistantPanel>,
    pub(super) has_project: bool,
    pub(super) is_remote_project: bool,
}
pub(in crate::app) fn workspace_view_hash<T: std::hash::Hash + ?Sized>(value: &T) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(value, &mut hasher);
    std::hash::Hasher::finish(&hasher)
}

pub(in crate::app) fn terminal_ai_titles_by_terminal_id(
    sessions: &[codux_runtime::ai_runtime_state::AIRuntimeSessionSummary],
) -> HashMap<String, (String, Option<String>)> {
    let mut titles = HashMap::new();
    let mut updated_at_by_terminal_id = HashMap::new();
    for session in sessions {
        let terminal_id = session.terminal_id.trim();
        if terminal_id.is_empty() {
            continue;
        }
        if updated_at_by_terminal_id
            .get(terminal_id)
            .is_some_and(|updated_at| session.updated_at < *updated_at)
        {
            continue;
        }
        updated_at_by_terminal_id.insert(terminal_id.to_string(), session.updated_at);
        titles.insert(
            terminal_id.to_string(),
            (
                terminal_ai_tool_title(&session.tool),
                session
                    .model
                    .as_deref()
                    .map(str::trim)
                    .filter(|model| !model.is_empty())
                    .map(str::to_string),
            ),
        );
    }
    titles
}

fn terminal_ai_tool_title(tool: &str) -> String {
    let tool = tool.trim();
    if tool.is_empty() {
        return "AI CLI".to_string();
    }
    tool.to_string()
}

// Local terminals spawn the user's login shell; its basename ("zsh") is the
// default label when nothing set a meaningful title.
fn default_shell_display_name() -> Option<&'static str> {
    static NAME: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    NAME.get_or_init(|| {
        if cfg!(windows) {
            return Some("powershell".to_string());
        }
        let shell = std::env::var("SHELL").ok()?;
        let name = std::path::Path::new(shell.trim()).file_stem()?;
        let name = name.to_string_lossy().trim().to_string();
        if name.is_empty() { None } else { Some(name) }
    })
    .as_deref()
}

// Title precedence: AI tool (+model subtitle) > custom pane label > shell OSC title > shell name > default label.
pub(in crate::app) fn terminal_pane_display_title(
    slot: &TerminalPaneSlot,
    ai_titles: &HashMap<String, (String, Option<String>)>,
    osc_title: Option<&str>,
    language: &str,
) -> (String, Option<String>) {
    if let Some((title, subtitle)) = slot
        .terminal_id
        .as_deref()
        .and_then(|terminal_id| ai_titles.get(terminal_id))
    {
        return (title.clone(), subtitle.clone());
    }
    let title = slot.title.trim();
    if !title.is_empty() && !terminal_slot_title_is_generic(title, language) {
        return (title.to_string(), None);
    }
    if let Some(osc_title) = osc_title.map(str::trim).filter(|title| !title.is_empty()) {
        return (friendly_osc_title(osc_title), None);
    }
    if let Some(shell) = default_shell_display_name() {
        return (shell.to_string(), None);
    }
    if !title.is_empty() {
        return (title.to_string(), None);
    }
    let locale = locale_from_language_setting(language);
    (translate(&locale, "terminal.title", "Terminal"), None)
}

// Auto-minted pane labels ("Terminal %d"/"Split %d", localized) are placeholders, not user titles.
fn terminal_slot_title_is_generic(title: &str, language: &str) -> bool {
    let locale = locale_from_language_setting(language);
    [
        translate(&locale, "terminal.default_format", "Terminal %d"),
        translate(&locale, "terminal.split.default_format", "Split %d"),
        "Terminal %d".to_string(),
        "Split %d".to_string(),
    ]
    .iter()
    .any(|format| title_matches_numbered_format(title, format))
}

// Windows conhost defaults the console title to the shell's exe path; show just the exe stem (cmd, pwsh, ...).
fn friendly_osc_title(title: &str) -> String {
    if !title.to_ascii_lowercase().ends_with(".exe") {
        return title.to_string();
    }
    // Backslash-split by hand so remote Windows titles also map when viewed from macOS.
    let normalized = title.replace('\\', "/");
    std::path::Path::new(&normalized)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(title)
        .to_string()
}

fn title_matches_numbered_format(title: &str, format: &str) -> bool {
    let Some((prefix, suffix)) = format.split_once("%d") else {
        return title == format;
    };
    title
        .strip_prefix(prefix)
        .and_then(|rest| rest.strip_suffix(suffix))
        .is_some_and(|digits| !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit()))
}
