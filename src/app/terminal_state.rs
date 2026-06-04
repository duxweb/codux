use super::types::{
    TerminalPanePlan, TerminalPaneSlot, TerminalRestorePlan, TerminalTab, TerminalTabPlan,
};
use crate::terminal::{
    ColorPalette, TerminalConfig, TerminalLaunchContext, TerminalPane,
    terminal_config_with_font_family,
};
use crate::theme;
use anyhow::Result;
use codux_runtime::{
    i18n::translate,
    memory::{MemoryLaunchRequest, MemoryService, launch_artifact_paths},
    runtime_bridge::RuntimeInventory,
    runtime_state::RuntimeState,
    settings::{AppSettingsStore, SettingsSummary, locale_from_language_setting},
    terminal_layout::{TerminalLayoutSummary, TerminalPaneSummary, TerminalTabSummary},
    terminal_pty::TerminalManager,
    terminal_runtime::{TerminalRuntimeSessionSummary, TerminalRuntimeSummary},
    tool_permissions::ToolPermissionsSummary,
    worktree::WorktreeInfo,
};
use gpui::px;
use std::{collections::HashSet, path::PathBuf, sync::Arc};
use uuid::Uuid;

#[cfg(test)]
pub(in crate::app) fn terminal_restore_plan(
    layout: &TerminalLayoutSummary,
    runtime: &TerminalRuntimeSummary,
) -> TerminalRestorePlan {
    terminal_restore_plan_for_language(layout, runtime, "simplifiedChinese")
}

pub(in crate::app) fn terminal_restore_plan_for_language(
    layout: &TerminalLayoutSummary,
    runtime: &TerminalRuntimeSummary,
    language: &str,
) -> TerminalRestorePlan {
    let locale = locale_from_language_setting(language);
    let tr = |key: &str, fallback: &str| translate(&locale, key, fallback);
    let tab_title = |index: usize| {
        tr("terminal.tab.default_format", "Terminal %d").replace("%d", &index.to_string())
    };
    let split_title = |index: usize| {
        tr("terminal.split.default_format", "Split %d").replace("%d", &index.to_string())
    };
    let mut tabs = Vec::new();
    let mut used_runtime_terminal_ids = HashSet::new();
    if !layout.top_panes.is_empty() {
        tabs.push(TerminalTabPlan {
            source_id: None,
            terminal_id: None,
            label: tr("terminal.main", "Main Terminal"),
            panes: layout
                .top_panes
                .iter()
                .enumerate()
                .map(|(index, pane)| {
                    let title = if pane.title.trim().is_empty() {
                        split_title(index + 1)
                    } else {
                        pane.title.clone()
                    };
                    let session = unique_runtime_session(
                        runtime_top_session(runtime, index),
                        &mut used_runtime_terminal_ids,
                    );
                    TerminalPanePlan {
                        source_id: session
                            .map(|session| session.slot_id.clone())
                            .filter(|id| !id.trim().is_empty()),
                        terminal_id: session
                            .map(|session| session.terminal_id.clone())
                            .filter(|id| !id.trim().is_empty()),
                        title,
                        restored_output_bytes: session
                            .map(|session| session.output_bytes)
                            .unwrap_or_default(),
                        restored_output_tail: session
                            .map(|session| session.output_tail.clone())
                            .unwrap_or_default(),
                    }
                })
                .collect(),
        });
    }
    tabs.extend(layout.tabs.iter().enumerate().map(|(index, tab)| {
        let label = if tab.label.trim().is_empty() {
            tab_title(index + 1)
        } else {
            tab.label.clone()
        };
        let session = unique_runtime_session(
            runtime_bottom_session(runtime, index, &tab.id),
            &mut used_runtime_terminal_ids,
        );
        TerminalTabPlan {
            source_id: Some(tab.id.clone()).filter(|id| !id.trim().is_empty()),
            terminal_id: None,
            panes: vec![TerminalPanePlan {
                source_id: session
                    .map(|session| session.slot_id.clone())
                    .filter(|id| !id.trim().is_empty()),
                terminal_id: session
                    .map(|session| session.terminal_id.clone())
                    .filter(|id| !id.trim().is_empty()),
                title: label.clone(),
                restored_output_bytes: session
                    .map(|session| session.output_bytes)
                    .unwrap_or_default(),
                restored_output_tail: session
                    .map(|session| session.output_tail.clone())
                    .unwrap_or_default(),
            }],
            label,
        }
    }));
    if tabs.is_empty() {
        let default_title = tab_title(1);
        tabs.push(TerminalTabPlan {
            source_id: None,
            terminal_id: None,
            label: default_title.clone(),
            panes: vec![TerminalPanePlan {
                source_id: None,
                terminal_id: None,
                title: default_title,
                restored_output_bytes: restored_terminal_output_bytes(runtime, None, None),
                restored_output_tail: restored_terminal_output_tail(runtime, None, None),
            }],
        });
    }
    for (index, tab) in tabs.iter_mut().enumerate() {
        if tab.panes.is_empty() {
            tab.panes.push(TerminalPanePlan {
                source_id: tab.source_id.clone(),
                terminal_id: tab.terminal_id.clone(),
                title: split_title(index + 1),
                restored_output_bytes: restored_terminal_output_bytes(
                    runtime,
                    tab.terminal_id.as_deref(),
                    tab.source_id.as_deref(),
                ),
                restored_output_tail: restored_terminal_output_tail(
                    runtime,
                    tab.terminal_id.as_deref(),
                    tab.source_id.as_deref(),
                ),
            });
        }
    }

    let active_index = layout
        .tabs
        .iter()
        .position(|tab| !layout.active_tab_id.is_empty() && tab.id == layout.active_tab_id)
        .map(|index| {
            if layout.top_panes.is_empty() {
                index
            } else {
                index + 1
            }
        })
        .or_else(|| {
            (!layout.top_panes.is_empty()
                && (layout.active_slot_id.is_empty()
                    || layout
                        .top_panes
                        .iter()
                        .any(|pane| pane.id == layout.active_slot_id)))
            .then_some(0)
        })
        .unwrap_or(0)
        .min(tabs.len().saturating_sub(1));

    TerminalRestorePlan { tabs, active_index }
}

pub(in crate::app) fn normalize_terminal_restore_state(
    owner_id: Option<&str>,
    mut layout: TerminalLayoutSummary,
    runtime: TerminalRuntimeSummary,
) -> (TerminalLayoutSummary, TerminalRuntimeSummary) {
    let Some(owner_id) = owner_id.filter(|id| !id.trim().is_empty()) else {
        return (layout, runtime);
    };

    layout = structural_terminal_layout(layout);
    if !layout
        .top_panes
        .iter()
        .any(|pane| pane.id == layout.active_slot_id)
        && !layout
            .tabs
            .iter()
            .any(|tab| tab.id == layout.active_slot_id)
    {
        layout.active_slot_id = layout
            .top_panes
            .first()
            .map(|pane| pane.id.clone())
            .or_else(|| layout.tabs.first().map(|tab| tab.id.clone()))
            .unwrap_or_default();
    }
    layout.active_tab_id = layout
        .tabs
        .iter()
        .find(|tab| tab.id == layout.active_slot_id)
        .map(|tab| tab.id.clone())
        .unwrap_or_default();

    let mut runtime = runtime_for_owner(runtime, owner_id);
    if !runtime
        .sessions
        .iter()
        .any(|session| session.terminal_id == runtime.active_terminal_id)
    {
        runtime.active_terminal_id.clear();
    }
    if !runtime.sessions.iter().any(|session| {
        session.slot_id == runtime.active_slot_id || session.tab_id == runtime.active_slot_id
    }) {
        runtime.active_slot_id.clear();
    }
    (layout, runtime)
}

pub(in crate::app) fn bottom_slot_id(owner_id: &str, index: usize) -> String {
    let _ = owner_id;
    format!("bottom-{}", index + 1)
}

pub(in crate::app) fn bottom_terminal_id(owner_id: &str, index: usize) -> String {
    let _ = index;
    unique_terminal_id(owner_id)
}

pub(in crate::app) fn top_slot_id(owner_id: &str, index: usize) -> String {
    let _ = index;
    unique_main_slot_id(owner_id)
}

pub(in crate::app) fn top_terminal_id(owner_id: &str, index: usize) -> String {
    let _ = index;
    unique_terminal_id(owner_id)
}

pub(in crate::app) fn structural_terminal_layout(
    mut layout: TerminalLayoutSummary,
) -> TerminalLayoutSummary {
    let old_active_slot_id = layout.active_slot_id.clone();
    let old_active_tab_id = layout.active_tab_id.clone();

    for (index, pane) in layout.top_panes.iter_mut().enumerate() {
        let old_id = pane.id.clone();
        pane.id = structural_top_slot_id(index);
        pane.terminal_id.clear();
        if old_active_slot_id == old_id {
            layout.active_slot_id = pane.id.clone();
        }
    }

    for (index, tab) in layout.tabs.iter_mut().enumerate() {
        let old_id = tab.id.clone();
        tab.id = structural_bottom_slot_id(index);
        tab.terminal_id.clear();
        if old_active_slot_id == old_id || old_active_tab_id == old_id {
            layout.active_slot_id = tab.id.clone();
            layout.active_tab_id = tab.id.clone();
        }
    }

    if !layout
        .top_panes
        .iter()
        .any(|pane| pane.id == layout.active_slot_id)
        && !layout
            .tabs
            .iter()
            .any(|tab| tab.id == layout.active_slot_id)
    {
        layout.active_slot_id = layout
            .top_panes
            .first()
            .map(|pane| pane.id.clone())
            .or_else(|| layout.tabs.first().map(|tab| tab.id.clone()))
            .unwrap_or_default();
    }
    layout.active_tab_id = layout
        .tabs
        .iter()
        .find(|tab| tab.id == layout.active_slot_id)
        .map(|tab| tab.id.clone())
        .unwrap_or_default();
    layout
}

fn structural_top_slot_id(index: usize) -> String {
    format!("main-{}", index + 1)
}

fn structural_bottom_slot_id(index: usize) -> String {
    format!("bottom-{}", index + 1)
}

fn unique_main_slot_id(owner_id: &str) -> String {
    format!("gpui-pane-{owner_id}-{}", Uuid::new_v4())
}

pub(in crate::app) fn unique_bottom_slot_id(owner_id: &str) -> String {
    format!("bottom-{owner_id}-{}", Uuid::new_v4())
}

fn unique_terminal_id(owner_id: &str) -> String {
    format!("gpui-term-{owner_id}-{}", Uuid::new_v4())
}

fn runtime_top_session(
    runtime: &TerminalRuntimeSummary,
    index: usize,
) -> Option<&TerminalRuntimeSessionSummary> {
    runtime
        .sessions
        .iter()
        .filter(|session| !session.slot_id.starts_with("bottom-"))
        .nth(index)
}

fn runtime_bottom_session<'a>(
    runtime: &'a TerminalRuntimeSummary,
    index: usize,
    structural_tab_id: &str,
) -> Option<&'a TerminalRuntimeSessionSummary> {
    runtime
        .sessions
        .iter()
        .filter(|session| session.slot_id.starts_with("bottom-"))
        .find(|session| session.tab_id == structural_tab_id)
        .or_else(|| {
            runtime
                .sessions
                .iter()
                .filter(|session| session.slot_id.starts_with("bottom-"))
                .nth(index)
        })
}

fn unique_runtime_session<'a>(
    session: Option<&'a TerminalRuntimeSessionSummary>,
    used_terminal_ids: &mut HashSet<String>,
) -> Option<&'a TerminalRuntimeSessionSummary> {
    let session = session?;
    if session.terminal_id.trim().is_empty()
        || !used_terminal_ids.insert(session.terminal_id.clone())
    {
        return None;
    }
    Some(session)
}

fn runtime_for_owner(
    mut runtime: TerminalRuntimeSummary,
    owner_id: &str,
) -> TerminalRuntimeSummary {
    runtime
        .sessions
        .retain(|session| terminal_session_belongs_to_owner(session, owner_id));
    runtime.open_count = runtime
        .sessions
        .iter()
        .filter(|session| session.is_running)
        .count();
    runtime.closed_count = runtime.sessions.len().saturating_sub(runtime.open_count);
    runtime
}

fn terminal_session_belongs_to_owner(
    session: &TerminalRuntimeSessionSummary,
    owner_id: &str,
) -> bool {
    let owner_id = owner_id.trim();
    if owner_id.is_empty() {
        return false;
    }
    if session.project_id.trim() == owner_id {
        return true;
    }
    let terminal_prefix = format!("gpui-term-{owner_id}-");
    let pane_prefix = format!("gpui-pane-{owner_id}-");
    let bottom_prefix = format!("bottom-{owner_id}-");
    session.terminal_id.starts_with(&terminal_prefix)
        || session.slot_id.starts_with(&pane_prefix)
        || session.slot_id.starts_with(&bottom_prefix)
        || session.tab_id.starts_with(&pane_prefix)
        || session.tab_id.starts_with(&bottom_prefix)
}

fn restored_terminal_output_tail(
    runtime: &TerminalRuntimeSummary,
    terminal_id: Option<&str>,
    slot_id: Option<&str>,
) -> String {
    runtime
        .sessions
        .iter()
        .find(|session| terminal_session_matches(session, terminal_id, slot_id))
        .map(|session| session.output_tail.clone())
        .unwrap_or_default()
}

fn restored_terminal_output_bytes(
    runtime: &TerminalRuntimeSummary,
    terminal_id: Option<&str>,
    slot_id: Option<&str>,
) -> usize {
    runtime
        .sessions
        .iter()
        .find(|session| terminal_session_matches(session, terminal_id, slot_id))
        .map(|session| session.output_bytes)
        .unwrap_or_default()
}

fn terminal_session_matches(
    session: &TerminalRuntimeSessionSummary,
    terminal_id: Option<&str>,
    slot_id: Option<&str>,
) -> bool {
    let terminal_id = terminal_id.filter(|id| !id.trim().is_empty());
    let slot_id = slot_id.filter(|id| !id.trim().is_empty());
    let terminal_matches = terminal_id.is_some_and(|id| session.terminal_id == id);
    let slot_matches = slot_id.is_some_and(|id| session.slot_id == id || session.tab_id == id);
    match (terminal_id, slot_id) {
        (Some(_), Some(_)) => terminal_matches && slot_matches,
        (Some(_), None) => terminal_matches,
        (None, Some(_)) => slot_matches,
        (None, None) => false,
    }
}

pub(in crate::app) fn spawn_terminal_tabs<C>(
    plan: &TerminalRestorePlan,
    terminal_manager: Arc<TerminalManager>,
    launch_context: Option<&TerminalLaunchContext>,
    terminal_config: TerminalConfig,
    cx: &mut C,
) -> Result<(Vec<TerminalTab>, usize, usize)>
where
    C: gpui::AppContext,
{
    let mut next_id = 1;
    let mut tabs = Vec::new();
    for (tab_plan_index, tab_plan) in plan.tabs.iter().enumerate() {
        let tab_id = next_id;
        next_id += 1;
        let mut panes = Vec::new();
        let mount_tab = tab_plan_index == plan.active_index
            || (tab_plan.source_id.is_none() && tab_plan.terminal_id.is_none());
        for (pane_index, pane_plan) in tab_plan.panes.iter().enumerate() {
            let mut pane_plan = pane_plan.clone();
            if pane_plan.source_id.is_none()
                && tab_plan.source_id.is_some()
                && let Some(base_context) = launch_context
            {
                pane_plan.source_id = Some(unique_bottom_slot_id(&base_context.project_id));
            }
            let pane_context =
                terminal_pane_launch_context(launch_context, tab_id, pane_index, &pane_plan);
            let pane = if mount_tab {
                Some(TerminalPane::spawn_with_context_and_config(
                    cx,
                    terminal_manager.clone(),
                    pane_context.as_ref(),
                    terminal_config.clone(),
                )?)
            } else {
                None
            };
            panes.push(TerminalPaneSlot {
                title: pane_plan.title.clone(),
                launch_context: pane_context.clone(),
                pane,
                restored_output_bytes: pane_plan.restored_output_bytes,
                restored_output_tail: pane_plan.restored_output_tail.clone(),
            });
        }
        if panes.is_empty() {
            let mut pane_plan = TerminalPanePlan {
                source_id: None,
                terminal_id: tab_plan.terminal_id.clone(),
                title: tab_plan.label.clone(),
                restored_output_bytes: 0,
                restored_output_tail: String::new(),
            };
            if tab_plan.source_id.is_some()
                && let Some(base_context) = launch_context
            {
                pane_plan.source_id = Some(unique_bottom_slot_id(&base_context.project_id));
            }
            let pane_context = terminal_pane_launch_context(launch_context, tab_id, 0, &pane_plan);
            let pane = if mount_tab {
                Some(TerminalPane::spawn_with_context_and_config(
                    cx,
                    terminal_manager.clone(),
                    pane_context.as_ref(),
                    terminal_config.clone(),
                )?)
            } else {
                None
            };
            panes.push(TerminalPaneSlot {
                title: tab_plan.label.clone(),
                launch_context: pane_context.clone(),
                pane,
                restored_output_bytes: pane_plan.restored_output_bytes,
                restored_output_tail: pane_plan.restored_output_tail,
            });
        }
        tabs.push(TerminalTab {
            id: tab_id,
            label: tab_plan.label.clone(),
            source_id: tab_plan.source_id.clone(),
            terminal_id: tab_plan.terminal_id.clone(),
            panes,
        });
    }
    let active_terminal_id = tabs
        .get(plan.active_index)
        .or_else(|| tabs.first())
        .map(|tab| tab.id)
        .unwrap_or(1);
    Ok((tabs, active_terminal_id, next_id))
}

pub(in crate::app) fn terminal_launch_context(
    state: &RuntimeState,
    runtime: &RuntimeInventory,
    tool_permissions: &ToolPermissionsSummary,
) -> Option<TerminalLaunchContext> {
    let project = state.selected_project.as_ref()?;
    let worktree = super::ai_runtime_status::selected_worktree_info(state);
    let workspace_id = worktree
        .as_ref()
        .map(|worktree| worktree.id.clone())
        .unwrap_or_else(|| project.id.clone());
    let workspace_name = worktree
        .as_ref()
        .filter(|worktree| !worktree.is_default)
        .map(|worktree| format!("{} · {}", project.name, worktree_row_context_name(worktree)))
        .unwrap_or_else(|| project.name.clone());
    let workspace_path = worktree
        .as_ref()
        .map(|worktree| worktree.path.clone())
        .unwrap_or_else(|| project.path.clone());
    let default_terminal_id = format!("gpui-term-{}-1", workspace_id);
    let default_slot_id = format!("gpui-pane-{}-1", workspace_id);
    let default_session_key = format!(
        "gpui:{}:{}:{}",
        workspace_id, default_terminal_id, default_slot_id
    );
    let default_session_instance_id =
        Uuid::new_v5(&Uuid::NAMESPACE_URL, default_session_key.as_bytes()).to_string();
    let memory_artifacts = (state.memory.available && state.settings.memory_enabled)
        .then(|| launch_artifact_paths(&workspace_id));
    Some(TerminalLaunchContext {
        project_id: workspace_id,
        project_name: workspace_name,
        project_path: PathBuf::from(workspace_path),
        support_dir: state.support_dir.clone(),
        runtime_root: runtime.root.clone(),
        terminal_id: Some(default_terminal_id),
        slot_id: Some(default_slot_id),
        session_key: Some(default_session_key),
        session_title: None,
        session_cwd: None,
        session_instance_id: Some(default_session_instance_id),
        tool_permissions_file: tool_permissions
            .error
            .is_none()
            .then(|| PathBuf::from(&tool_permissions.path)),
        memory_workspace_root: memory_artifacts
            .as_ref()
            .map(|artifacts| artifacts.workspace_root.clone()),
        memory_prompt_file: memory_artifacts
            .as_ref()
            .map(|artifacts| artifacts.prompt_file.clone()),
        memory_index_file: memory_artifacts.map(|artifacts| artifacts.index_file),
    })
}

fn worktree_row_context_name(worktree: &WorktreeInfo) -> String {
    let name = worktree.name.trim();
    if !name.is_empty() {
        return name.to_string();
    }
    let branch = worktree.branch.trim();
    if branch.is_empty() {
        "main".to_string()
    } else {
        branch
            .split('/')
            .filter(|segment| !segment.is_empty())
            .next_back()
            .unwrap_or(branch)
            .to_string()
    }
}

pub(in crate::app) fn terminal_config_for_settings(settings: &SettingsSummary) -> TerminalConfig {
    let mut config = terminal_config_with_font_family(&settings.terminal_font_family);
    let font_size = settings
        .terminal_font_size
        .parse::<f32>()
        .unwrap_or(14.0)
        .clamp(10.0, 28.0);
    config.font_size = px(font_size);
    config.scrollback = settings
        .terminal_scrollback_lines
        .parse::<usize>()
        .unwrap_or(config.scrollback)
        .clamp(200, 10_000);
    config.colors = terminal_color_palette(&settings.theme, &settings.theme_color);
    config
}

fn terminal_color_palette(theme_name: &str, theme_color: &str) -> ColorPalette {
    let palette = theme::terminal_theme_palette(theme_name);
    let accent = theme::theme_color_value(theme_color);
    let rgb = |hex: u32| -> (u8, u8, u8) {
        (
            ((hex >> 16) & 0xff) as u8,
            ((hex >> 8) & 0xff) as u8,
            (hex & 0xff) as u8,
        )
    };
    let (br, bg, bb) = rgb(palette.background);
    let (fr, fg, fb) = rgb(palette.foreground);
    let cursor_hex = if theme_color.trim().is_empty() {
        palette.cursor
    } else {
        accent
    };
    let (cr, cg, cb) = rgb(cursor_hex);
    let (sr, sg, sb) = rgb(palette.selection);
    let (r0, g0, b0) = rgb(palette.black);
    let (r1, g1, b1) = rgb(palette.red);
    let (r2, g2, b2) = rgb(palette.green);
    let (r3, g3, b3) = rgb(palette.yellow);
    let (r4, g4, b4) = rgb(palette.blue);
    let (r5, g5, b5) = rgb(palette.magenta);
    let (r6, g6, b6) = rgb(palette.cyan);
    let (r7, g7, b7) = rgb(palette.white);
    let (r8, g8, b8) = rgb(palette.bright_black);
    let (r9, g9, b9) = rgb(palette.bright_red);
    let (r10, g10, b10) = rgb(palette.bright_green);
    let (r11, g11, b11) = rgb(palette.bright_yellow);
    let (r12, g12, b12) = rgb(palette.bright_blue);
    let (r13, g13, b13) = rgb(palette.bright_magenta);
    let (r14, g14, b14) = rgb(palette.bright_cyan);
    let (r15, g15, b15) = rgb(palette.bright_white);
    ColorPalette::builder()
        .background(br, bg, bb)
        .foreground(fr, fg, fb)
        .cursor(cr, cg, cb)
        .selection(sr, sg, sb)
        .black(r0, g0, b0)
        .red(r1, g1, b1)
        .green(r2, g2, b2)
        .yellow(r3, g3, b3)
        .blue(r4, g4, b4)
        .magenta(r5, g5, b5)
        .cyan(r6, g6, b6)
        .white(r7, g7, b7)
        .bright_black(r8, g8, b8)
        .bright_red(r9, g9, b9)
        .bright_green(r10, g10, b10)
        .bright_yellow(r11, g11, b11)
        .bright_blue(r12, g12, b12)
        .bright_magenta(r13, g13, b13)
        .bright_cyan(r14, g14, b14)
        .bright_white(r15, g15, b15)
        .build()
}

pub(in crate::app) fn terminal_pane_launch_context(
    base: Option<&TerminalLaunchContext>,
    _tab_id: usize,
    _pane_index: usize,
    pane: &TerminalPanePlan,
) -> Option<TerminalLaunchContext> {
    let mut context = base.cloned()?;
    let terminal_id = pane
        .terminal_id
        .clone()
        .filter(|id| !id.trim().is_empty())
        .map(|id| project_terminal_id(&context.project_id, &id))
        .unwrap_or_else(|| unique_terminal_id(&context.project_id));
    let slot_id = pane
        .source_id
        .clone()
        .filter(|id| !id.trim().is_empty())
        .map(|id| project_slot_id(&context.project_id, &id))
        .unwrap_or_else(|| unique_main_slot_id(&context.project_id));
    let session_key = format!("gpui:{}:{terminal_id}:{slot_id}", context.project_id);
    let session_instance_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, session_key.as_bytes());
    context.terminal_id = Some(terminal_id);
    context.slot_id = Some(slot_id);
    context.session_key = Some(session_key);
    context.session_title = Some(pane.title.clone());
    context.session_cwd = Some(context.project_path.clone());
    context.session_instance_id = Some(session_instance_id.to_string());
    Some(context)
}

fn project_terminal_id(project_id: &str, terminal_id: &str) -> String {
    if terminal_id.starts_with(&format!("gpui-term-{project_id}-")) {
        terminal_id.to_string()
    } else if let Some(suffix) = terminal_id.strip_prefix("gpui-term-") {
        format!("gpui-term-{project_id}-{suffix}")
    } else {
        terminal_id.to_string()
    }
}

fn project_slot_id(project_id: &str, slot_id: &str) -> String {
    if slot_id.starts_with(&format!("gpui-pane-{project_id}-")) {
        slot_id.to_string()
    } else if let Some(suffix) = slot_id.strip_prefix("gpui-pane-") {
        format!("gpui-pane-{project_id}-{suffix}")
    } else {
        slot_id.to_string()
    }
}

pub(in crate::app) fn prepare_memory_launch_artifacts(state: &RuntimeState) {
    if !state.memory.available || !state.settings.memory_enabled {
        return;
    }
    let Some(project) = &state.selected_project else {
        return;
    };
    let app_settings = AppSettingsStore::from_support_dir(state.support_dir.clone()).snapshot();
    let _ = MemoryService::new(state.support_dir.clone()).prepare_launch_artifacts(
        MemoryLaunchRequest {
            project_id: project.id.clone(),
            project_name: project.name.clone(),
            workspace_path: Some(project.path.clone()),
            settings: app_settings.ai,
            extra_context: None,
        },
    );
}

pub(in crate::app) fn terminal_tab_summary(index: usize, tab: &TerminalTab) -> TerminalTabSummary {
    TerminalTabSummary {
        id: structural_bottom_slot_id(index),
        label: tab.label.clone(),
        terminal_id: String::new(),
    }
}

pub(in crate::app) fn terminal_pane_summary(
    index: usize,
    slot: &TerminalPaneSlot,
) -> TerminalPaneSummary {
    TerminalPaneSummary {
        id: structural_top_slot_id(index),
        title: slot.title.clone(),
        terminal_id: String::new(),
    }
}
