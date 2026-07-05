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
    memory::launch_artifact_paths,
    runtime_bridge::RuntimeInventory,
    runtime_state::{RuntimeService, RuntimeState},
    settings::{SettingsSummary, locale_from_language_setting},
    terminal_layout::{
        SplitAxis, TerminalLayoutSummary, TerminalPaneSummary, TerminalSplitNode, TerminalTopGrid,
        normalize_split_ratios, normalize_split_tree, normalize_top_grid, single_row_top_grid,
        split_tree_leaf_count, top_grid_from_split_tree,
    },
    terminal_pty::{TerminalManager, TerminalOutputSnapshot, TerminalPtyConfig},
    terminal_runtime::{TerminalRuntimeSessionSummary, TerminalRuntimeSummary},
    tool_permissions::ToolPermissionsSummary,
    worktree::WorktreeInfo,
};
use gpui::{WindowAppearance, px};
use std::sync::Arc;
use std::{collections::HashMap, path::PathBuf};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum TerminalSplitDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::app) enum TerminalSplitScope {
    Inner,
    Root,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::app) struct TerminalSplitLocation {
    pub(in crate::app) parent_path: Vec<usize>,
    pub(in crate::app) child_index: usize,
    pub(in crate::app) parent_axis: Option<SplitAxis>,
    pub(in crate::app) pane_index: usize,
}

#[cfg(test)]
pub(in crate::app) fn terminal_restore_plan(
    layout: &TerminalLayoutSummary,
    runtime: &TerminalRuntimeSummary,
) -> TerminalRestorePlan {
    terminal_restore_plan_for_language(layout, runtime, "simplifiedChinese", None)
}

pub(in crate::app) fn terminal_restore_plan_for_language(
    layout: &TerminalLayoutSummary,
    runtime: &TerminalRuntimeSummary,
    language: &str,
    active_terminal_id: Option<String>,
) -> TerminalRestorePlan {
    let mut layout = layout.clone();
    migrate_legacy_tabs_to_top_panes(&mut layout);
    let locale = locale_from_language_setting(language);
    let tr = |key: &str, fallback: &str| translate(&locale, key, fallback);
    let terminal_title = |index: usize| {
        tr("terminal.default_format", "Terminal %d").replace("%d", &index.to_string())
    };
    let split_title = |index: usize| {
        tr("terminal.split.default_format", "Split %d").replace("%d", &index.to_string())
    };
    let mut tabs = Vec::new();
    if !layout.top_panes.is_empty() {
        tabs.push(TerminalTabPlan {
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
    if tabs.is_empty() {
        let default_title = terminal_title(1);
        tabs.push(TerminalTabPlan {
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

    TerminalRestorePlan {
        tabs,
        active_index,
        active_terminal_id,
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
    // Drop any tab/pane carried over from ANOTHER workspace. A terminal id is
    // owned by exactly one owner (`gpui-term-{owner}-…`); a foreign-owner id can
    // leak into this restore during a laggy project switch (e.g. via the runtime
    // layout cache) and would otherwise resolve to the OTHER project's live pane
    // — the cross-talk ("串台") where switching projects shows the wrong terminal.
    layout
        .top_panes
        .retain(|pane| !terminal_id_is_foreign_to_owner(&pane.terminal_id, owner_id));
    layout
        .collapsed_panes
        .retain(|pane| !terminal_id_is_foreign_to_owner(&pane.terminal_id, owner_id));
    layout.tabs.clear();
    if layout.top_panes.is_empty() {
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

pub(in crate::app) fn top_terminal_id(owner_id: &str, index: usize) -> String {
    let _ = index;
    unique_terminal_id(owner_id)
}

pub(in crate::app) fn structural_terminal_layout(
    mut layout: TerminalLayoutSummary,
) -> TerminalLayoutSummary {
    migrate_legacy_tabs_to_top_panes(&mut layout);
    layout
        .top_panes
        .retain(|pane| !pane.terminal_id.trim().is_empty());
    layout
        .collapsed_panes
        .retain(|pane| !pane.terminal_id.trim().is_empty());
    layout.tabs.clear();
    layout.top_ratios =
        terminal_top_ratios_for_panes(layout.top_ratios.clone(), layout.top_panes.len());
    layout.top_grid = terminal_top_grid_for_panes(
        layout.top_grid.clone(),
        &layout.top_ratios,
        layout.top_panes.len(),
    );
    layout.split_tree = terminal_split_tree_for_panes(
        layout.split_tree.clone(),
        &layout.top_grid,
        &layout.top_ratios,
        layout.top_panes.len(),
    );
    layout.top_grid = layout
        .split_tree
        .as_ref()
        .map(|tree| top_grid_from_split_tree(tree, layout.top_panes.len()))
        .unwrap_or_else(TerminalTopGrid::default);
    layout.top_ratios = terminal_top_ratios_from_grid(&layout.top_grid);
    layout.active_terminal_id.clear();
    layout
}

fn migrate_legacy_tabs_to_top_panes(layout: &mut TerminalLayoutSummary) {
    let mut seen = layout
        .top_panes
        .iter()
        .map(|pane| pane.terminal_id.trim().to_string())
        .filter(|terminal_id| !terminal_id.is_empty())
        .collect::<std::collections::HashSet<_>>();
    for tab in &layout.tabs {
        let terminal_id = tab.terminal_id.trim();
        if terminal_id.is_empty() || !seen.insert(terminal_id.to_string()) {
            continue;
        }
        let title = if tab.label.trim().is_empty() {
            "Terminal"
        } else {
            tab.label.trim()
        };
        layout.top_panes.push(TerminalPaneSummary {
            title: title.to_string(),
            terminal_id: terminal_id.to_string(),
        });
    }
}

pub(in crate::app) fn default_terminal_layout_for_owner(
    owner_id: Option<&str>,
    language: &str,
) -> TerminalLayoutSummary {
    let locale = locale_from_language_setting(language);
    let title = translate(&locale, "terminal.default_format", "Terminal %d").replace("%d", "1");
    let terminal_id = owner_id
        .filter(|id| !id.trim().is_empty())
        .map(|id| top_terminal_id(id, 0))
        .unwrap_or_default();
    TerminalLayoutSummary {
        active_terminal_id: terminal_id.clone(),
        top_panes: vec![TerminalPaneSummary { title, terminal_id }],
        top_ratios: vec![1.0],
        top_grid: terminal_single_row_grid_from_ratios(vec![1.0], 1),
        split_tree: Some(TerminalSplitNode::Leaf { pane: 0 }),
        bottom_ratio: DEFAULT_TERMINAL_BOTTOM_RATIO,
        error: None,
        ..TerminalLayoutSummary::default()
    }
}

pub(in crate::app) const DEFAULT_TERMINAL_BOTTOM_RATIO: f64 = 0.24;

pub(in crate::app) fn terminal_top_ratios_for_panes(
    ratios: Vec<f64>,
    pane_count: usize,
) -> Vec<f64> {
    if pane_count == 0 {
        return Vec::new();
    }
    // Saved ratios only describe the panes that existed when the user dragged
    // the divider. When the pane count changes (a split was added or closed),
    // the saved split no longer maps onto the new panes — so a freshly added
    // split opens evenly instead of inheriting a skewed share of the old layout.
    if ratios.len() != pane_count {
        return vec![1.0 / pane_count as f64; pane_count];
    }
    let values = ratios
        .into_iter()
        .map(|value| {
            if value.is_finite() {
                value.max(0.0)
            } else {
                0.0
            }
        })
        .collect::<Vec<_>>();
    let total = values.iter().sum::<f64>();
    if total <= 0.0 {
        return vec![1.0 / pane_count as f64; pane_count];
    }
    values.into_iter().map(|value| value / total).collect()
}

pub(in crate::app) fn terminal_top_grid_for_panes(
    grid: TerminalTopGrid,
    top_ratios: &[f64],
    pane_count: usize,
) -> TerminalTopGrid {
    normalize_top_grid(grid, top_ratios, pane_count)
}

pub(in crate::app) fn terminal_single_row_grid_from_ratios(
    ratios: Vec<f64>,
    pane_count: usize,
) -> TerminalTopGrid {
    single_row_top_grid(ratios, pane_count)
}

pub(in crate::app) fn terminal_top_ratios_from_grid(grid: &TerminalTopGrid) -> Vec<f64> {
    grid.columns.iter().map(|column| column.ratio).collect()
}

pub(in crate::app) fn terminal_split_tree_for_panes(
    tree: Option<TerminalSplitNode>,
    fallback_grid: &TerminalTopGrid,
    top_ratios: &[f64],
    pane_count: usize,
) -> Option<TerminalSplitNode> {
    normalize_split_tree(tree, fallback_grid, top_ratios, pane_count)
}

pub(in crate::app) fn terminal_split_tree_equal(
    left: &Option<TerminalSplitNode>,
    right: &Option<TerminalSplitNode>,
) -> bool {
    fn equal_node(left: &TerminalSplitNode, right: &TerminalSplitNode) -> bool {
        match (left, right) {
            (TerminalSplitNode::Leaf { pane: left }, TerminalSplitNode::Leaf { pane: right }) => {
                left == right
            }
            (
                TerminalSplitNode::Split {
                    axis: left_axis,
                    ratios: left_ratios,
                    children: left_children,
                },
                TerminalSplitNode::Split {
                    axis: right_axis,
                    ratios: right_ratios,
                    children: right_children,
                },
            ) => {
                left_axis == right_axis
                    && left_children.len() == right_children.len()
                    && left_ratios.len() == right_ratios.len()
                    && left_ratios
                        .iter()
                        .zip(right_ratios)
                        .all(|(left, right)| (left - right).abs() < 0.001)
                    && left_children
                        .iter()
                        .zip(right_children)
                        .all(|(left, right)| equal_node(left, right))
            }
            _ => false,
        }
    }
    match (left, right) {
        (Some(left), Some(right)) => equal_node(left, right),
        (None, None) => true,
        _ => false,
    }
}

pub(in crate::app) fn terminal_split_tree_location_for_pane(
    tree: &TerminalSplitNode,
    pane_index: usize,
) -> Option<TerminalSplitLocation> {
    fn walk(
        node: &TerminalSplitNode,
        pane_index: usize,
        path: &mut Vec<usize>,
        parent_axis: Option<SplitAxis>,
        child_index: usize,
    ) -> Option<TerminalSplitLocation> {
        match node {
            TerminalSplitNode::Leaf { pane } => {
                (*pane == pane_index).then(|| TerminalSplitLocation {
                    parent_path: path.clone(),
                    child_index,
                    parent_axis,
                    pane_index,
                })
            }
            TerminalSplitNode::Split { axis, children, .. } => {
                for (index, child) in children.iter().enumerate() {
                    path.push(index);
                    let found = walk(child, pane_index, path, Some(*axis), index);
                    path.pop();
                    if let Some(location) = found {
                        return Some(location);
                    }
                }
                None
            }
        }
    }
    walk(tree, pane_index, &mut Vec::new(), None, 0)
}

pub(in crate::app) fn terminal_split_tree_insert_pane(
    tree: &TerminalSplitNode,
    source_index: usize,
    new_pane_index: usize,
    direction: TerminalSplitDirection,
) -> Result<TerminalSplitNode, &'static str> {
    let split_count = split_tree_leaf_count(tree);
    if split_count >= codux_runtime::terminal_layout::TERMINAL_SPLIT_CAP {
        return Err("main split limit reached");
    }
    let axis = split_axis_for_direction(direction);
    let before = matches!(
        direction,
        TerminalSplitDirection::Left | TerminalSplitDirection::Up
    );
    let mut next = tree.clone();
    let source_after_insert = if new_pane_index <= source_index {
        source_index + 1
    } else {
        source_index
    };
    increment_panes_from(&mut next, new_pane_index);
    let inserted =
        insert_split_at_leaf(&mut next, source_after_insert, new_pane_index, axis, before);
    if !inserted {
        return Err("无效分屏位置");
    }
    Ok(normalize_app_split_tree(next, split_count + 1))
}

pub(in crate::app) fn terminal_split_tree_insert_pane_root(
    tree: &TerminalSplitNode,
    new_pane_index: usize,
    direction: TerminalSplitDirection,
) -> Result<TerminalSplitNode, &'static str> {
    let split_count = split_tree_leaf_count(tree);
    if split_count >= codux_runtime::terminal_layout::TERMINAL_SPLIT_CAP {
        return Err("main split limit reached");
    }
    let axis = split_axis_for_direction(direction);
    let before = matches!(
        direction,
        TerminalSplitDirection::Left | TerminalSplitDirection::Up
    );
    let mut next = tree.clone();
    increment_panes_from(&mut next, new_pane_index);
    let new_leaf = TerminalSplitNode::Leaf {
        pane: new_pane_index,
    };
    next = match next {
        TerminalSplitNode::Split {
            axis: root_axis,
            mut children,
            ..
        } if root_axis == axis => {
            if before {
                children.insert(0, new_leaf);
            } else {
                children.push(new_leaf);
            }
            TerminalSplitNode::Split {
                axis,
                ratios: even_split_ratios(children.len()),
                children,
            }
        }
        other => TerminalSplitNode::Split {
            axis,
            ratios: vec![0.5, 0.5],
            children: if before {
                vec![new_leaf, other]
            } else {
                vec![other, new_leaf]
            },
        },
    };
    Ok(normalize_app_split_tree(next, split_count + 1))
}

pub(in crate::app) fn terminal_split_tree_remove_pane(
    tree: &TerminalSplitNode,
    pane_index: usize,
) -> Option<TerminalSplitNode> {
    let leaf_count = split_tree_leaf_count(tree);
    if leaf_count <= 1 {
        return Some(tree.clone());
    }
    let mut next = tree.clone();
    if !remove_split_leaf(&mut next, pane_index) {
        return Some(tree.clone());
    }
    decrement_panes_after(&mut next, pane_index);
    Some(normalize_app_split_tree(next, leaf_count - 1))
}

pub(in crate::app) fn terminal_split_tree_with_restored_location(
    tree: &TerminalSplitNode,
    location: Option<TerminalSplitLocation>,
    new_pane_index: usize,
) -> Result<(TerminalSplitNode, usize), &'static str> {
    if split_tree_leaf_count(tree) >= codux_runtime::terminal_layout::TERMINAL_SPLIT_CAP {
        return Err("main split limit reached");
    }
    if let Some(location) = location {
        let mut next = tree.clone();
        increment_panes_from(&mut next, new_pane_index);
        if insert_restored_leaf(
            &mut next,
            &location.parent_path,
            location.child_index,
            new_pane_index,
        ) {
            return Ok((
                normalize_app_split_tree(next, split_tree_leaf_count(tree) + 1),
                new_pane_index,
            ));
        }
    }
    let source = split_tree_leaf_count(tree).saturating_sub(1);
    terminal_split_tree_insert_pane(tree, source, new_pane_index, TerminalSplitDirection::Right)
        .map(|tree| (tree, new_pane_index))
}

pub(in crate::app) fn terminal_split_tree_update_ratios(
    tree: &TerminalSplitNode,
    path: &[usize],
    ratios: Vec<f64>,
) -> TerminalSplitNode {
    let mut next = tree.clone();
    update_split_ratios_at_path(&mut next, path, ratios);
    normalize_app_split_tree(next, split_tree_leaf_count(tree))
}

fn normalize_app_split_tree(tree: TerminalSplitNode, pane_count: usize) -> TerminalSplitNode {
    let fallback = top_grid_from_split_tree(&tree, pane_count);
    normalize_split_tree(Some(tree), &fallback, &vec![1.0; pane_count], pane_count)
        .unwrap_or_else(|| TerminalSplitNode::Leaf { pane: 0 })
}

fn split_axis_for_direction(direction: TerminalSplitDirection) -> SplitAxis {
    match direction {
        TerminalSplitDirection::Left | TerminalSplitDirection::Right => SplitAxis::Horizontal,
        TerminalSplitDirection::Up | TerminalSplitDirection::Down => SplitAxis::Vertical,
    }
}

fn insert_split_at_leaf(
    node: &mut TerminalSplitNode,
    source_index: usize,
    new_pane_index: usize,
    axis: SplitAxis,
    before: bool,
) -> bool {
    match node {
        TerminalSplitNode::Leaf { pane } if *pane == source_index => {
            let old = TerminalSplitNode::Leaf { pane: *pane };
            let new = TerminalSplitNode::Leaf {
                pane: new_pane_index,
            };
            let children = if before {
                vec![new, old]
            } else {
                vec![old, new]
            };
            *node = TerminalSplitNode::Split {
                axis,
                ratios: vec![0.5, 0.5],
                children,
            };
            true
        }
        TerminalSplitNode::Leaf { .. } => false,
        TerminalSplitNode::Split {
            axis: parent_axis,
            ratios,
            children,
        } => {
            for index in 0..children.len() {
                if split_tree_contains_pane(&children[index], source_index) {
                    if *parent_axis == axis {
                        let insert_index = if before { index } else { index + 1 };
                        children.insert(
                            insert_index,
                            TerminalSplitNode::Leaf {
                                pane: new_pane_index,
                            },
                        );
                        *ratios = even_split_ratios(children.len());
                        return true;
                    }
                    return insert_split_at_leaf(
                        &mut children[index],
                        source_index,
                        new_pane_index,
                        axis,
                        before,
                    );
                }
            }
            false
        }
    }
}

fn remove_split_leaf(node: &mut TerminalSplitNode, pane_index: usize) -> bool {
    match node {
        TerminalSplitNode::Leaf { .. } => false,
        TerminalSplitNode::Split {
            ratios, children, ..
        } => {
            if let Some(index) = children.iter().position(
                |child| matches!(child, TerminalSplitNode::Leaf { pane } if *pane == pane_index),
            ) {
                children.remove(index);
                if index < ratios.len() {
                    ratios.remove(index);
                }
                *ratios = even_split_ratios(children.len());
                return true;
            }
            for child in children {
                if remove_split_leaf(child, pane_index) {
                    return true;
                }
            }
            false
        }
    }
}

fn insert_restored_leaf(
    node: &mut TerminalSplitNode,
    parent_path: &[usize],
    child_index: usize,
    pane_index: usize,
) -> bool {
    if parent_path.is_empty() {
        return false;
    }
    let parent_path = &parent_path[..parent_path.len() - 1];
    let Some(parent) = split_node_at_path_mut(node, parent_path) else {
        return false;
    };
    match parent {
        TerminalSplitNode::Split {
            ratios, children, ..
        } => {
            let insert_index = child_index.min(children.len());
            children.insert(insert_index, TerminalSplitNode::Leaf { pane: pane_index });
            *ratios = even_split_ratios(children.len());
            true
        }
        TerminalSplitNode::Leaf { .. } => false,
    }
}

fn even_split_ratios(count: usize) -> Vec<f64> {
    if count == 0 {
        Vec::new()
    } else {
        vec![1.0 / count as f64; count]
    }
}

fn update_split_ratios_at_path(
    node: &mut TerminalSplitNode,
    path: &[usize],
    next_ratios: Vec<f64>,
) -> bool {
    let Some(node) = split_node_at_path_mut(node, path) else {
        return false;
    };
    if let TerminalSplitNode::Split {
        ratios, children, ..
    } = node
    {
        *ratios = normalize_split_ratios(next_ratios, children.len());
        true
    } else {
        false
    }
}

fn split_node_at_path_mut<'a>(
    node: &'a mut TerminalSplitNode,
    path: &[usize],
) -> Option<&'a mut TerminalSplitNode> {
    let mut current = node;
    for index in path {
        match current {
            TerminalSplitNode::Split { children, .. } => {
                current = children.get_mut(*index)?;
            }
            TerminalSplitNode::Leaf { .. } => return None,
        }
    }
    Some(current)
}

fn split_tree_contains_pane(node: &TerminalSplitNode, pane_index: usize) -> bool {
    match node {
        TerminalSplitNode::Leaf { pane } => *pane == pane_index,
        TerminalSplitNode::Split { children, .. } => children
            .iter()
            .any(|child| split_tree_contains_pane(child, pane_index)),
    }
}

fn increment_panes_from(node: &mut TerminalSplitNode, start: usize) {
    match node {
        TerminalSplitNode::Leaf { pane } => {
            if *pane >= start {
                *pane += 1;
            }
        }
        TerminalSplitNode::Split { children, .. } => {
            for child in children {
                increment_panes_from(child, start);
            }
        }
    }
}

fn decrement_panes_after(node: &mut TerminalSplitNode, removed: usize) {
    match node {
        TerminalSplitNode::Leaf { pane } => {
            if *pane > removed {
                *pane -= 1;
            }
        }
        TerminalSplitNode::Split { children, .. } => {
            for child in children {
                decrement_panes_after(child, removed);
            }
        }
    }
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

/// Whether `terminal_id` was minted for a DIFFERENT workspace than `owner_id`.
/// Terminal ids are `gpui-term-{owner}-…`; an id carrying the `gpui-term-`
/// prefix but a different owner segment belongs to another project/worktree and
/// must not be restored here. Non-`gpui-term-` ids are left alone (not ours).
fn terminal_id_is_foreign_to_owner(terminal_id: &str, owner_id: &str) -> bool {
    let owner_id = owner_id.trim();
    if owner_id.is_empty() {
        return false;
    }
    terminal_id.starts_with("gpui-term-")
        && !terminal_id.starts_with(&format!("gpui-term-{owner_id}-"))
}

/// Whether `layout` carries any pane terminal id minted for a DIFFERENT
/// owner than `owner_id` — i.e. this layout doesn't belong to that workspace.
pub(in crate::app) fn terminal_layout_is_foreign_to_owner(
    layout: &TerminalLayoutSummary,
    owner_id: &str,
) -> bool {
    layout
        .top_panes
        .iter()
        .chain(layout.collapsed_panes.iter())
        .map(|pane| pane.terminal_id.as_str())
        .any(|terminal_id| terminal_id_is_foreign_to_owner(terminal_id, owner_id))
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
    mut pending_out: Option<&mut Vec<(TerminalPtyConfig, crate::terminal::PendingTerminalAttach)>>,
    cx: &mut C,
) -> Result<(Vec<TerminalTab>, usize, usize)>
where
    C: gpui::AppContext,
{
    let (mut tabs, active_terminal_id, next_id) =
        restore_terminal_tabs_skeleton(plan, launch_context);
    for tab_index in 0..tabs.len() {
        let Some(tab) = tabs.get_mut(tab_index) else {
            continue;
        };
        mount_terminal_tab_panes(
            tab,
            terminal_manager.clone(),
            base_pty_config,
            &terminal_config,
            terminal_pane_registry,
            pending_out.as_deref_mut(),
            cx,
        )?;
    }
    Ok((tabs, active_terminal_id, next_id))
}

fn mount_terminal_tab_panes<C>(
    tab: &mut TerminalTab,
    terminal_manager: Arc<TerminalManager>,
    base_pty_config: &TerminalPtyConfig,
    terminal_config: &TerminalConfig,
    terminal_pane_registry: &HashMap<String, TerminalPane>,
    mut pending_out: Option<&mut Vec<(TerminalPtyConfig, crate::terminal::PendingTerminalAttach)>>,
    cx: &mut C,
) -> Result<()>
where
    C: gpui::AppContext,
{
    for slot in tab.panes.iter_mut() {
        if slot.pane.is_some() {
            continue;
        }
        let pty_config = terminal_pty_config_for_terminal_id(
            base_pty_config,
            slot.terminal_id.as_deref(),
            &slot.title,
        );
        if let Some(pane) = slot
            .terminal_id
            .as_deref()
            .and_then(|terminal_id| terminal_pane_registry.get(terminal_id))
            .filter(|pane| pane.matches_pty_config(&pty_config))
            .cloned()
        {
            refresh_terminal_pane_config(&pane, terminal_config, cx);
            slot.pane = Some(pane);
            continue;
        }
        // A remote-hosted project's terminal must open on the host. When the
        // caller can drive the async attach chokepoint (`pending_out`), build a
        // pending pane and defer — opening it inline would block the UI thread
        // on a network round-trip. Local terminals (and the boot path, which has
        // no pending sink yet) spawn the PTY synchronously as before.
        if pty_config.host_device_id.is_some() {
            if let Some(out) = pending_out.as_deref_mut() {
                let (pane, attach) = TerminalPane::pending_with_restored_output(
                    cx,
                    pty_config.clone(),
                    terminal_config.clone(),
                    Some(TerminalOutputSnapshot {
                        bytes: slot.restored_output_bytes,
                        tail: slot.restored_output_tail.clone(),
                    }),
                );
                slot.pane = Some(pane);
                out.push((pty_config, attach));
                continue;
            }
        }
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
            terminal_id: tab_plan.terminal_id.clone(),
            panes,
        });
    }
    let active_terminal_id = tabs
        .get(plan.active_index)
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
    let launch_artifacts = launch_artifact_paths(&workspace_id);
    Some(TerminalLaunchContext {
        root_project_id: project.id.clone(),
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
        memory_workspace_root: Some(launch_artifacts.workspace_root),
        memory_prompt_file: Some(launch_artifacts.prompt_file),
        memory_index_file: Some(launch_artifacts.index_file),
        host_device_id: project.host_device_id.clone(),
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
    config.language = locale_from_language_setting(&settings.language);
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

fn project_terminal_id(owner_id: &str, terminal_id: &str) -> String {
    if terminal_id.starts_with(&format!("gpui-term-{owner_id}-")) {
        terminal_id.to_string()
    } else if terminal_id.starts_with("gpui-term-") {
        // A `gpui-term-` id minted for a DIFFERENT owner. The old behavior
        // re-prefixed it (`gpui-term-{owner}-{foreign}-…`), which accreted a new
        // project segment on every switch AND, un-mangled, could resolve to the
        // other project's live pane (cross-talk). A foreign id can't be restored
        // under this owner — mint a fresh one instead.
        unique_terminal_id(owner_id)
    } else {
        terminal_id.to_string()
    }
}

pub(in crate::app) fn prepare_memory_launch_artifacts(
    service: &RuntimeService,
    state: &RuntimeState,
) {
    let Some(project) = &state.selected_project else {
        return;
    };
    let _ = service.prepare_memory_launch_artifacts(&project.id, &project.name, &project.path);
}

pub(in crate::app) fn terminal_pane_summary(slot: &TerminalPaneSlot) -> TerminalPaneSummary {
    TerminalPaneSummary {
        title: slot.title.clone(),
        terminal_id: slot.terminal_id.clone().unwrap_or_default(),
    }
}

pub(in crate::app) fn collapsed_terminal_slots_from_layout(
    layout: &TerminalLayoutSummary,
    runtime: &TerminalRuntimeSummary,
    filter_dead_sessions: bool,
    terminal_pane_registry: &HashMap<String, TerminalPane>,
    terminal_manager: &Arc<TerminalManager>,
) -> Vec<TerminalPaneSlot> {
    let visible_terminal_ids = layout
        .top_panes
        .iter()
        .map(|pane| pane.terminal_id.as_str())
        .collect::<std::collections::HashSet<_>>();
    layout
        .collapsed_panes
        .iter()
        .filter(|pane| {
            let terminal_id = pane.terminal_id.trim();
            !terminal_id.is_empty() && !visible_terminal_ids.contains(terminal_id)
        })
        .filter(|pane| {
            if !filter_dead_sessions {
                return true;
            }
            collapsed_terminal_session_is_live(
                &pane.terminal_id,
                runtime,
                terminal_pane_registry,
                terminal_manager,
            )
        })
        .map(|pane| {
            let terminal_id = pane.terminal_id.trim().to_string();
            let session = runtime_session_by_terminal_id(runtime, &terminal_id);
            TerminalPaneSlot {
                title: if pane.title.trim().is_empty() {
                    "Terminal".to_string()
                } else {
                    pane.title.clone()
                },
                terminal_id: Some(terminal_id),
                pane: None,
                restored_output_bytes: session
                    .map(|session| session.output_bytes)
                    .unwrap_or_default(),
                restored_output_tail: session
                    .map(|session| session.output_tail.clone())
                    .unwrap_or_default(),
            }
        })
        .collect()
}

fn collapsed_terminal_session_is_live(
    terminal_id: &str,
    runtime: &TerminalRuntimeSummary,
    terminal_pane_registry: &HashMap<String, TerminalPane>,
    terminal_manager: &Arc<TerminalManager>,
) -> bool {
    let terminal_id = terminal_id.trim();
    if terminal_id.is_empty() {
        return false;
    }
    if terminal_pane_registry.contains_key(terminal_id) {
        return true;
    }
    if runtime_session_by_terminal_id(runtime, terminal_id).is_some() {
        return true;
    }
    terminal_manager.output_snapshot(terminal_id).is_ok()
}
