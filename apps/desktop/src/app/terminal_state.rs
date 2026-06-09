use super::types::{
    TerminalPanePlan, TerminalPaneSlot, TerminalRestorePlan, TerminalTab, TerminalTabPlacement,
    TerminalTabPlan,
};
use crate::terminal::{
    ColorPalette, TerminalConfig, TerminalLaunchContext, TerminalPane,
    terminal_config_with_font_family,
};
use crate::theme;
use anyhow::Result;
use codux_runtime::{
    i18n::translate,
    memory::launch_artifact_paths,
    runtime_bridge::RuntimeInventory,
    runtime_state::{RuntimeService, RuntimeState},
    settings::{SettingsSummary, locale_from_language_setting},
    terminal_layout::{TerminalLayoutSummary, TerminalPaneSummary, TerminalTabSummary},
    terminal_pty::{TerminalManager, TerminalPtyConfig},
    terminal_runtime::{TerminalRuntimeSessionSummary, TerminalRuntimeSummary},
    tool_permissions::ToolPermissionsSummary,
    worktree::WorktreeInfo,
};
use gpui::{WindowAppearance, px};
use std::sync::Arc;
use std::{collections::HashMap, path::PathBuf};
use uuid::Uuid;

#[cfg(test)]
pub(in crate::app) fn terminal_restore_plan(
    layout: &TerminalLayoutSummary,
    runtime: &TerminalRuntimeSummary,
) -> TerminalRestorePlan {
    terminal_restore_plan_for_language(layout, runtime, "simplifiedChinese", None, None)
}

pub(in crate::app) fn terminal_restore_plan_for_language(
    layout: &TerminalLayoutSummary,
    runtime: &TerminalRuntimeSummary,
    language: &str,
    active_terminal_id: Option<String>,
    active_bottom_terminal_id: Option<String>,
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
    if !layout.top_panes.is_empty() {
        tabs.push(TerminalTabPlan {
            placement: TerminalTabPlacement::Top,
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
                    let terminal_id = normalized_terminal_id(&pane.terminal_id);
                    let session = terminal_id
                        .as_deref()
                        .and_then(|id| runtime_session_by_terminal_id(runtime, id));
                    TerminalPanePlan {
                        terminal_id: terminal_id.or_else(|| {
                            session
                                .map(|session| session.terminal_id.clone())
                                .filter(|id| !id.trim().is_empty())
                        }),
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
        let terminal_id = normalized_terminal_id(&tab.terminal_id);
        let session = terminal_id
            .as_deref()
            .and_then(|id| runtime_session_by_terminal_id(runtime, id));
        TerminalTabPlan {
            placement: TerminalTabPlacement::Bottom,
            terminal_id: terminal_id.clone(),
            panes: vec![TerminalPanePlan {
                terminal_id: terminal_id.or_else(|| {
                    session
                        .map(|session| session.terminal_id.clone())
                        .filter(|id| !id.trim().is_empty())
                }),
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
            placement: TerminalTabPlacement::Top,
            terminal_id: None,
            label: default_title.clone(),
            panes: vec![TerminalPanePlan {
                terminal_id: None,
                title: default_title,
                restored_output_bytes: 0,
                restored_output_tail: String::new(),
            }],
        });
    }
    for (index, tab) in tabs.iter_mut().enumerate() {
        if tab.panes.is_empty() {
            tab.panes.push(TerminalPanePlan {
                terminal_id: tab.terminal_id.clone(),
                title: split_title(index + 1),
                restored_output_bytes: restored_terminal_output_bytes(
                    runtime,
                    tab.terminal_id.as_deref(),
                ),
                restored_output_tail: restored_terminal_output_tail(
                    runtime,
                    tab.terminal_id.as_deref(),
                ),
            });
        }
    }

    let active_terminal_id = active_terminal_id
        .and_then(|terminal_id| normalized_terminal_id(&terminal_id))
        .filter(|terminal_id| restore_plan_has_terminal(&tabs, terminal_id));
    let active_index = active_terminal_id
        .as_deref()
        .and_then(|terminal_id| active_terminal_plan_index(&tabs, terminal_id))
        .unwrap_or(0)
        .min(tabs.len().saturating_sub(1));
    let active_bottom_terminal_id = active_bottom_terminal_id
        .and_then(|terminal_id| normalized_terminal_id(&terminal_id))
        .filter(|terminal_id| {
            tabs.iter().any(|tab| {
                tab.placement == TerminalTabPlacement::Bottom
                    && tab.terminal_id.as_deref() == Some(terminal_id.as_str())
            })
        });

    TerminalRestorePlan {
        tabs,
        active_index,
        active_terminal_id,
        active_bottom_terminal_id,
    }
}

pub(in crate::app) fn normalize_terminal_restore_state(
    owner_id: Option<&str>,
    mut layout: TerminalLayoutSummary,
    runtime: TerminalRuntimeSummary,
    language: &str,
) -> (TerminalLayoutSummary, TerminalRuntimeSummary) {
    let Some(owner_id) = owner_id.filter(|id| !id.trim().is_empty()) else {
        return (layout, runtime);
    };

    layout = structural_terminal_layout(layout);
    if layout.top_panes.is_empty() && layout.tabs.is_empty() {
        layout = default_terminal_layout_for_owner(Some(owner_id), language);
    }
    layout.active_terminal_id.clear();

    let mut runtime = runtime_for_owner(runtime, owner_id);
    if !runtime
        .sessions
        .iter()
        .any(|session| session.terminal_id == runtime.active_terminal_id)
    {
        runtime.active_terminal_id.clear();
    }
    (layout, runtime)
}

pub(in crate::app) fn bottom_terminal_id(owner_id: &str, index: usize) -> String {
    let _ = index;
    unique_terminal_id(owner_id)
}

pub(in crate::app) fn top_terminal_id(owner_id: &str, index: usize) -> String {
    let _ = index;
    unique_terminal_id(owner_id)
}

pub(in crate::app) fn structural_terminal_layout(
    mut layout: TerminalLayoutSummary,
) -> TerminalLayoutSummary {
    layout
        .top_panes
        .retain(|pane| !pane.terminal_id.trim().is_empty());
    layout.tabs.retain(|tab| !tab.terminal_id.trim().is_empty());
    layout.active_terminal_id.clear();
    layout
}

pub(in crate::app) fn default_terminal_layout_for_owner(
    owner_id: Option<&str>,
    language: &str,
) -> TerminalLayoutSummary {
    let locale = locale_from_language_setting(language);
    let title = translate(&locale, "terminal.tab.default_format", "Terminal %d").replace("%d", "1");
    let terminal_id = owner_id
        .filter(|id| !id.trim().is_empty())
        .map(|id| top_terminal_id(id, 0))
        .unwrap_or_default();
    TerminalLayoutSummary {
        active_terminal_id: terminal_id.clone(),
        top_panes: vec![TerminalPaneSummary { title, terminal_id }],
        top_ratios: vec![1.0],
        bottom_ratio: DEFAULT_TERMINAL_BOTTOM_RATIO,
        error: None,
        ..TerminalLayoutSummary::default()
    }
}

pub(in crate::app) const DEFAULT_TERMINAL_BOTTOM_RATIO: f64 = 0.24;

pub(in crate::app) fn clamp_terminal_bottom_ratio(value: f64) -> f64 {
    if !value.is_finite() {
        return DEFAULT_TERMINAL_BOTTOM_RATIO;
    }
    value.clamp(0.16, 0.58)
}

pub(in crate::app) fn terminal_top_ratios_for_panes(
    ratios: Vec<f64>,
    pane_count: usize,
) -> Vec<f64> {
    if pane_count == 0 {
        return Vec::new();
    }
    let mut values = ratios
        .into_iter()
        .take(pane_count)
        .map(|value| {
            if value.is_finite() {
                value.max(0.0)
            } else {
                0.0
            }
        })
        .collect::<Vec<_>>();
    while values.len() < pane_count {
        values.push(1.0 / pane_count as f64);
    }
    let total = values.iter().sum::<f64>();
    if total <= 0.0 {
        return vec![1.0 / pane_count as f64; pane_count];
    }
    values.into_iter().map(|value| value / total).collect()
}

fn restore_plan_has_terminal(tabs: &[TerminalTabPlan], terminal_id: &str) -> bool {
    let terminal_id = terminal_id.trim();
    !terminal_id.is_empty()
        && tabs.iter().any(|tab| {
            tab.terminal_id.as_deref() == Some(terminal_id)
                || tab
                    .panes
                    .iter()
                    .any(|pane| pane.terminal_id.as_deref() == Some(terminal_id))
        })
}

fn active_terminal_plan_index(tabs: &[TerminalTabPlan], terminal_id: &str) -> Option<usize> {
    let terminal_id = terminal_id.trim();
    if terminal_id.is_empty() {
        return None;
    }
    tabs.iter().position(|tab| {
        tab.terminal_id.as_deref() == Some(terminal_id)
            || tab
                .panes
                .iter()
                .any(|pane| pane.terminal_id.as_deref() == Some(terminal_id))
    })
}

fn unique_terminal_id(owner_id: &str) -> String {
    format!("gpui-term-{owner_id}-{}", Uuid::new_v4())
}

fn normalized_terminal_id(terminal_id: &str) -> Option<String> {
    let terminal_id = terminal_id.trim();
    if terminal_id.is_empty() {
        None
    } else {
        Some(terminal_id.to_string())
    }
}

fn runtime_session_by_terminal_id<'a>(
    runtime: &'a TerminalRuntimeSummary,
    terminal_id: &str,
) -> Option<&'a TerminalRuntimeSessionSummary> {
    runtime
        .sessions
        .iter()
        .find(|session| session.terminal_id == terminal_id)
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
    session.terminal_id.starts_with(&terminal_prefix)
}

fn restored_terminal_output_tail(
    runtime: &TerminalRuntimeSummary,
    terminal_id: Option<&str>,
) -> String {
    runtime
        .sessions
        .iter()
        .find(|session| terminal_id.is_some_and(|id| session.terminal_id == id))
        .map(|session| session.output_tail.clone())
        .unwrap_or_default()
}

fn restored_terminal_output_bytes(
    runtime: &TerminalRuntimeSummary,
    terminal_id: Option<&str>,
) -> usize {
    runtime
        .sessions
        .iter()
        .find(|session| terminal_id.is_some_and(|id| session.terminal_id == id))
        .map(|session| session.output_bytes)
        .unwrap_or_default()
}

pub(in crate::app) fn spawn_terminal_tabs<C>(
    plan: &TerminalRestorePlan,
    terminal_manager: Arc<TerminalManager>,
    launch_context: Option<&TerminalLaunchContext>,
    base_pty_config: &TerminalPtyConfig,
    terminal_config: TerminalConfig,
    terminal_pane_registry: &HashMap<String, TerminalPane>,
    cx: &mut C,
) -> Result<(Vec<TerminalTab>, usize, usize)>
where
    C: gpui::AppContext,
{
    let (mut tabs, active_terminal_id, next_id) =
        restore_terminal_tabs_skeleton(plan, launch_context);
    for (tab_plan_index, tab_plan) in plan.tabs.iter().enumerate() {
        let mount_tab = tab_plan_index == plan.active_index
            || tabs
                .get(tab_plan_index)
                .is_some_and(|tab| tab.id == active_terminal_id)
            || tab_plan.placement == TerminalTabPlacement::Top;
        if mount_tab && let Some(tab) = tabs.get_mut(tab_plan_index) {
            mount_terminal_tab_panes(
                tab,
                terminal_manager.clone(),
                base_pty_config,
                &terminal_config,
                terminal_pane_registry,
                cx,
            )?;
        }
    }
    Ok((tabs, active_terminal_id, next_id))
}

fn mount_terminal_tab_panes<C>(
    tab: &mut TerminalTab,
    terminal_manager: Arc<TerminalManager>,
    base_pty_config: &TerminalPtyConfig,
    terminal_config: &TerminalConfig,
    terminal_pane_registry: &HashMap<String, TerminalPane>,
    cx: &mut C,
) -> Result<()>
where
    C: gpui::AppContext,
{
    for slot in &mut tab.panes {
        if slot.pane.is_some() {
            continue;
        }
        if let Some(pane) = slot
            .terminal_id
            .as_deref()
            .and_then(|terminal_id| terminal_pane_registry.get(terminal_id))
            .cloned()
        {
            refresh_terminal_pane_config(&pane, terminal_config, cx);
            slot.pane = Some(pane);
            continue;
        }
        let pty_config = terminal_pty_config_for_terminal_id(
            base_pty_config,
            slot.terminal_id.as_deref(),
            &slot.title,
        );
        slot.pane = Some(TerminalPane::spawn_with_pty_config(
            cx,
            terminal_manager.clone(),
            pty_config,
            terminal_config.clone(),
        )?);
    }
    Ok(())
}

pub(in crate::app) fn refresh_terminal_pane_config<C>(
    pane: &TerminalPane,
    terminal_config: &TerminalConfig,
    cx: &mut C,
) where
    C: gpui::AppContext,
{
    let config = terminal_config.clone();
    pane.view.update(cx, |terminal, cx| {
        terminal.update_config(config, cx);
    });
}

pub(in crate::app) fn terminal_pty_config_for_terminal_id(
    base: &TerminalPtyConfig,
    terminal_id: Option<&str>,
    title: &str,
) -> TerminalPtyConfig {
    let mut config = base.clone();
    config.slot_id = None;
    if let Some(terminal_id) = terminal_id.filter(|id| !id.trim().is_empty()) {
        config.terminal_id = Some(terminal_id.to_string());
        if let Some(project_id) = config.project_id.clone() {
            let session_key = format!("gpui:{project_id}:{terminal_id}");
            let session_instance_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, session_key.as_bytes());
            config.session_key = Some(session_key);
            config.session_instance_id = Some(session_instance_id.to_string());
        }
    } else if let (Some(project_id), Some(terminal_id)) =
        (config.project_id.clone(), config.terminal_id.clone())
    {
        let session_key = format!("gpui:{project_id}:{terminal_id}");
        let session_instance_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, session_key.as_bytes());
        config.session_key = Some(session_key);
        config.session_instance_id = Some(session_instance_id.to_string());
    }
    config.title = Some(title.to_string());
    config
}

pub(in crate::app) fn restore_terminal_tabs_skeleton(
    plan: &TerminalRestorePlan,
    launch_context: Option<&TerminalLaunchContext>,
) -> (Vec<TerminalTab>, usize, usize) {
    let mut next_id = 1;
    let mut tabs = Vec::new();
    for tab_plan in &plan.tabs {
        let view_id = next_id;
        next_id += 1;
        let mut panes = Vec::new();
        for pane_plan in &tab_plan.panes {
            let terminal_id = terminal_pane_terminal_id(launch_context, pane_plan);
            panes.push(TerminalPaneSlot {
                title: pane_plan.title.clone(),
                terminal_id,
                pane: None,
                restored_output_bytes: pane_plan.restored_output_bytes,
                restored_output_tail: pane_plan.restored_output_tail.clone(),
            });
        }
        if panes.is_empty() {
            let pane_plan = TerminalPanePlan {
                terminal_id: tab_plan.terminal_id.clone(),
                title: tab_plan.label.clone(),
                restored_output_bytes: 0,
                restored_output_tail: String::new(),
            };
            let terminal_id = terminal_pane_terminal_id(launch_context, &pane_plan);
            panes.push(TerminalPaneSlot {
                title: tab_plan.label.clone(),
                terminal_id,
                pane: None,
                restored_output_bytes: pane_plan.restored_output_bytes,
                restored_output_tail: pane_plan.restored_output_tail,
            });
        }
        tabs.push(TerminalTab {
            id: view_id,
            label: tab_plan.label.clone(),
            placement: tab_plan.placement,
            terminal_id: tab_plan.terminal_id.clone(),
            panes,
        });
    }
    let active_terminal_id = plan
        .active_bottom_terminal_id
        .as_deref()
        .and_then(|terminal_id| {
            tabs.iter().find(|tab| {
                tab.placement == TerminalTabPlacement::Bottom
                    && tab.terminal_id.as_deref() == Some(terminal_id)
            })
        })
        .or_else(|| {
            tabs.get(plan.active_index)
                .filter(|tab| tab.placement == TerminalTabPlacement::Bottom)
        })
        .or_else(|| {
            tabs.iter()
                .find(|tab| tab.placement == TerminalTabPlacement::Bottom)
        })
        .or_else(|| tabs.get(plan.active_index))
        .or_else(|| tabs.first())
        .map(|tab| tab.id)
        .unwrap_or(1);
    (tabs, active_terminal_id, next_id)
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
    let default_session_key = format!("gpui:{}:{}", workspace_id, default_terminal_id);
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
        slot_id: None,
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

pub(in crate::app) fn terminal_config_for_settings(
    settings: &SettingsSummary,
    appearance: WindowAppearance,
) -> TerminalConfig {
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
    config.paste_images_as_paths = settings.terminal_paste_images_as_paths;
    config.colors = terminal_color_palette(&settings.theme, &settings.theme_color, appearance);
    config
}

fn terminal_color_palette(
    theme_name: &str,
    theme_color: &str,
    appearance: WindowAppearance,
) -> ColorPalette {
    let palette = theme::terminal_theme_palette_for_appearance(theme_name, appearance);
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

pub(in crate::app) fn terminal_pane_terminal_id(
    base: Option<&TerminalLaunchContext>,
    pane: &TerminalPanePlan,
) -> Option<String> {
    let context = base?;
    Some(
        pane.terminal_id
            .clone()
            .filter(|id| !id.trim().is_empty())
            .map(|id| project_terminal_id(&context.project_id, &id))
            .unwrap_or_else(|| unique_terminal_id(&context.project_id)),
    )
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

pub(in crate::app) fn prepare_memory_launch_artifacts(
    service: &RuntimeService,
    state: &RuntimeState,
) {
    if !state.memory.available || !state.settings.memory_enabled {
        return;
    }
    let Some(project) = &state.selected_project else {
        return;
    };
    let _ = service.prepare_memory_launch_artifacts(&project.id, &project.name, &project.path);
}

pub(in crate::app) fn terminal_tab_summary(tab: &TerminalTab) -> TerminalTabSummary {
    TerminalTabSummary {
        label: tab.label.clone(),
        terminal_id: tab
            .panes
            .first()
            .and_then(|slot| slot.terminal_id.clone())
            .or_else(|| tab.terminal_id.clone())
            .unwrap_or_default(),
    }
}

pub(in crate::app) fn terminal_pane_summary(slot: &TerminalPaneSlot) -> TerminalPaneSummary {
    TerminalPaneSummary {
        title: slot.title.clone(),
        terminal_id: slot.terminal_id.clone().unwrap_or_default(),
    }
}
