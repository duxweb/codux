use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const TERMINAL_LAYOUT_NAMESPACE: &str = "terminal-layout";
const DEFAULT_BOTTOM_RATIO: f64 = 0.24;

pub fn terminal_layout_storage_key(project_id: &str, worktree_id: &str) -> String {
    codux_terminal_core::runtime_scope_key(project_id, Some(worktree_id))
}

/// Max columns a desktop user can stack in the main terminal grid.
pub const TERMINAL_GRID_MAX_COLUMNS: usize = 6;
/// Max rows a desktop user can stack inside one terminal grid column.
pub const TERMINAL_GRID_MAX_ROWS: usize = 6;
/// Max split panes in the main terminal grid.
pub const TERMINAL_SPLIT_CAP: usize = TERMINAL_GRID_MAX_COLUMNS * TERMINAL_GRID_MAX_ROWS;
const TERMINAL_SPLIT_MIN_RATIO: f64 = 0.01;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalLayoutSummary {
    #[serde(default, skip_serializing)]
    pub active_terminal_id: String,
    pub top_panes: Vec<TerminalPaneSummary>,
    pub tabs: Vec<TerminalTabSummary>,
    /// Legacy column fallback for old layouts; `top_grid` is authoritative for current layouts.
    #[serde(default)]
    pub top_ratios: Vec<f64>,
    #[serde(default)]
    pub top_grid: TerminalTopGrid,
    #[serde(default)]
    pub split_tree: Option<TerminalSplitNode>,
    #[serde(default = "default_bottom_ratio")]
    pub bottom_ratio: f64,
    #[serde(default)]
    pub collapsed_panes: Vec<TerminalPaneSummary>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalTopGrid {
    #[serde(default)]
    pub columns: Vec<TerminalGridColumn>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalGridColumn {
    pub ratio: f64,
    pub rows: usize,
    #[serde(default)]
    pub row_ratios: Vec<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SplitAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum TerminalSplitNode {
    Leaf {
        pane: usize,
    },
    Split {
        axis: SplitAxis,
        #[serde(default)]
        ratios: Vec<f64>,
        #[serde(default)]
        children: Vec<TerminalSplitNode>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalPaneSummary {
    pub title: String,
    pub terminal_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalTabSummary {
    pub label: String,
    pub terminal_id: String,
}

pub struct TerminalLayoutService {
    support_dir: PathBuf,
}

impl TerminalLayoutService {
    pub fn new(support_dir: PathBuf) -> Self {
        Self { support_dir }
    }

    pub fn load(&self, project_id: Option<&str>) -> TerminalLayoutSummary {
        let Some(project_id) = project_id else {
            return TerminalLayoutSummary {
                error: Some("No selected project.".to_string()),
                ..Default::default()
            };
        };
        if let Some(layout) = self.cache_layout(project_id) {
            return sanitize_terminal_layout(layout).unwrap_or_default();
        }
        TerminalLayoutSummary {
            bottom_ratio: default_bottom_ratio(),
            error: Some("No terminal layout saved for selected project.".to_string()),
            ..Default::default()
        }
    }

    fn cache_layout(&self, project_id: &str) -> Option<TerminalLayoutSummary> {
        crate::persistent_cache::PersistentCacheStore::for_support_dir(self.support_dir.clone())
            .ok()?
            .get_json::<TerminalLayoutSummary>(TERMINAL_LAYOUT_NAMESPACE, project_id)
            .ok()
            .flatten()
    }

    pub fn load_many<'a, I>(
        &self,
        project_ids: I,
    ) -> std::collections::HashMap<String, TerminalLayoutSummary>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let cache = crate::persistent_cache::PersistentCacheStore::for_support_dir(
            self.support_dir.clone(),
        )
        .ok();
        project_ids
            .into_iter()
            .filter_map(|project_id| {
                let layout = cache.as_ref().and_then(|cache| {
                    cache
                        .get_json::<TerminalLayoutSummary>(TERMINAL_LAYOUT_NAMESPACE, project_id)
                        .ok()
                        .flatten()
                })?;
                sanitize_terminal_layout(layout).map(|layout| (project_id.to_string(), layout))
            })
            .collect()
    }

    pub fn save_from_gpui(
        &self,
        project_id: &str,
        tabs: Vec<TerminalTabSummary>,
        top_panes: Vec<TerminalPaneSummary>,
        top_ratios: Vec<f64>,
        bottom_ratio: f64,
    ) -> Result<TerminalLayoutSummary, String> {
        self.save_from_gpui_with_grid(
            project_id,
            tabs,
            top_panes,
            top_ratios,
            TerminalTopGrid::default(),
            None,
            bottom_ratio,
            Vec::new(),
        )
    }

    pub fn save_from_gpui_with_grid(
        &self,
        project_id: &str,
        tabs: Vec<TerminalTabSummary>,
        top_panes: Vec<TerminalPaneSummary>,
        top_ratios: Vec<f64>,
        top_grid: TerminalTopGrid,
        split_tree: Option<TerminalSplitNode>,
        bottom_ratio: f64,
        collapsed_panes: Vec<TerminalPaneSummary>,
    ) -> Result<TerminalLayoutSummary, String> {
        if tabs.is_empty() && top_panes.is_empty() {
            return Err("Terminal layout is empty.".to_string());
        }
        let layout = TerminalLayoutSummary {
            tabs,
            active_terminal_id: String::new(),
            top_panes,
            top_ratios,
            top_grid,
            split_tree,
            bottom_ratio,
            collapsed_panes,
            error: None,
        };
        self.save_summary(project_id, layout)
    }

    pub fn save_summary(
        &self,
        project_id: &str,
        layout: TerminalLayoutSummary,
    ) -> Result<TerminalLayoutSummary, String> {
        let layout = sanitize_terminal_layout(layout)
            .ok_or_else(|| "Terminal layout is empty.".to_string())?;
        crate::persistent_cache::PersistentCacheStore::for_support_dir(self.support_dir.clone())?
            .put_json(TERMINAL_LAYOUT_NAMESPACE, project_id, &layout)?;
        Ok(layout)
    }

    pub fn delete(&self, project_id: &str) -> Result<bool, String> {
        crate::persistent_cache::PersistentCacheStore::for_support_dir(self.support_dir.clone())?
            .delete_json(TERMINAL_LAYOUT_NAMESPACE, project_id)
    }
}

pub(crate) fn terminal_layout_cache_namespace() -> &'static str {
    TERMINAL_LAYOUT_NAMESPACE
}

fn default_bottom_ratio() -> f64 {
    DEFAULT_BOTTOM_RATIO
}

fn sanitize_terminal_layout(mut layout: TerminalLayoutSummary) -> Option<TerminalLayoutSummary> {
    migrate_legacy_tabs_to_top_panes(&mut layout);
    layout.tabs.clear();
    if layout.top_panes.is_empty() {
        return None;
    }
    layout.top_ratios = normalize_ratios(layout.top_ratios, layout.top_panes.len());
    let fallback_grid =
        normalize_top_grid(layout.top_grid, &layout.top_ratios, layout.top_panes.len());
    let split_tree = normalize_split_tree(
        layout.split_tree.take(),
        &fallback_grid,
        &layout.top_ratios,
        layout.top_panes.len(),
    );
    layout.top_grid = split_tree
        .as_ref()
        .map(|tree| top_grid_from_split_tree(tree, layout.top_panes.len()))
        .unwrap_or(fallback_grid);
    layout.top_ratios = layout
        .top_grid
        .columns
        .iter()
        .map(|column| column.ratio)
        .collect();
    layout.split_tree = split_tree;
    layout.bottom_ratio = clamp_ratio(layout.bottom_ratio, 0.16, 0.58, default_bottom_ratio());
    layout
        .collapsed_panes
        .retain(|pane| !pane.terminal_id.trim().is_empty());
    Some(layout)
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

pub fn normalize_top_grid(
    grid: TerminalTopGrid,
    top_ratios: &[f64],
    pane_count: usize,
) -> TerminalTopGrid {
    if pane_count == 0 {
        return TerminalTopGrid::default();
    }
    if grid.columns.is_empty() {
        return single_row_top_grid(top_ratios.to_vec(), pane_count);
    }
    if grid.columns.iter().any(|column| column.rows == 0) {
        return single_row_top_grid(top_ratios.to_vec(), pane_count);
    }
    let total_rows = grid.columns.iter().map(|column| column.rows).sum::<usize>();
    if total_rows != pane_count {
        return single_row_top_grid(top_ratios.to_vec(), pane_count);
    }
    let column_count = grid.columns.len();
    let column_ratios = normalize_ratios(
        grid.columns
            .iter()
            .map(|column| column.ratio)
            .collect::<Vec<_>>(),
        column_count,
    );
    let columns = grid
        .columns
        .into_iter()
        .zip(column_ratios)
        .map(|(column, ratio)| {
            let rows = column.rows.max(1);
            TerminalGridColumn {
                ratio,
                rows,
                row_ratios: normalize_ratios(column.row_ratios, rows),
            }
        })
        .collect::<Vec<_>>();
    TerminalTopGrid { columns }
}

pub fn single_row_top_grid(ratios: Vec<f64>, pane_count: usize) -> TerminalTopGrid {
    if pane_count == 0 {
        return TerminalTopGrid::default();
    }
    let ratios = normalize_ratios(ratios, pane_count);
    TerminalTopGrid {
        columns: ratios
            .into_iter()
            .map(|ratio| TerminalGridColumn {
                ratio,
                rows: 1,
                row_ratios: vec![1.0],
            })
            .collect(),
    }
}

pub fn normalize_split_tree(
    tree: Option<TerminalSplitNode>,
    fallback_grid: &TerminalTopGrid,
    top_ratios: &[f64],
    pane_count: usize,
) -> Option<TerminalSplitNode> {
    if pane_count == 0 {
        return None;
    }
    if let Some(tree) = tree {
        let mut seen = std::collections::HashSet::new();
        if let Some(tree) = sanitize_split_node(tree, pane_count, &mut seen) {
            if seen.len() == pane_count && split_tree_leaf_count(&tree) == pane_count {
                return Some(tree);
            }
        }
    }
    Some(split_tree_from_top_grid(
        normalize_top_grid(fallback_grid.clone(), top_ratios, pane_count),
        pane_count,
    ))
}

pub fn split_tree_from_top_grid(grid: TerminalTopGrid, pane_count: usize) -> TerminalSplitNode {
    if pane_count == 0 {
        return TerminalSplitNode::Leaf { pane: 0 };
    }
    let grid = normalize_top_grid(grid, &vec![1.0 / pane_count as f64; pane_count], pane_count);
    let mut next_pane = 0usize;
    let mut children = Vec::new();
    let mut ratios = Vec::new();
    for column in grid.columns {
        let rows = column.rows.min(pane_count.saturating_sub(next_pane));
        if rows == 0 {
            continue;
        }
        let child = if rows == 1 {
            let pane = next_pane;
            next_pane += 1;
            TerminalSplitNode::Leaf { pane }
        } else {
            let row_ratios = normalize_ratios(column.row_ratios, rows);
            let row_children = (0..rows)
                .map(|_| {
                    let pane = next_pane;
                    next_pane += 1;
                    TerminalSplitNode::Leaf { pane }
                })
                .collect();
            TerminalSplitNode::Split {
                axis: SplitAxis::Vertical,
                ratios: row_ratios,
                children: row_children,
            }
        };
        children.push(child);
        ratios.push(column.ratio);
    }
    while next_pane < pane_count {
        children.push(TerminalSplitNode::Leaf { pane: next_pane });
        ratios.push(1.0);
        next_pane += 1;
    }
    split_from_children(SplitAxis::Horizontal, ratios, children)
        .unwrap_or(TerminalSplitNode::Leaf { pane: 0 })
}

pub fn top_grid_from_split_tree(tree: &TerminalSplitNode, pane_count: usize) -> TerminalTopGrid {
    if pane_count == 0 {
        return TerminalTopGrid::default();
    }
    if let Some(grid) = compatible_top_grid_from_split_tree(tree, pane_count) {
        return normalize_top_grid(grid, &vec![1.0 / pane_count as f64; pane_count], pane_count);
    }
    single_row_top_grid(vec![1.0; pane_count], pane_count)
}

pub fn split_tree_leaf_count(tree: &TerminalSplitNode) -> usize {
    match tree {
        TerminalSplitNode::Leaf { .. } => 1,
        TerminalSplitNode::Split { children, .. } => {
            children.iter().map(split_tree_leaf_count).sum()
        }
    }
}

fn sanitize_split_node(
    node: TerminalSplitNode,
    pane_count: usize,
    seen: &mut std::collections::HashSet<usize>,
) -> Option<TerminalSplitNode> {
    match node {
        TerminalSplitNode::Leaf { pane } => {
            if pane < pane_count && seen.insert(pane) {
                Some(TerminalSplitNode::Leaf { pane })
            } else {
                None
            }
        }
        TerminalSplitNode::Split {
            axis,
            ratios,
            children,
        } => {
            let mut clean_children = Vec::new();
            let mut clean_ratios = Vec::new();
            let ratios = normalize_split_ratios(ratios, children.len());
            for (child, ratio) in children.into_iter().zip(ratios) {
                let Some(child) = sanitize_split_node(child, pane_count, seen) else {
                    continue;
                };
                match child {
                    TerminalSplitNode::Split {
                        axis: child_axis,
                        ratios: child_ratios,
                        children: grand_children,
                    } if child_axis == axis => {
                        let child_ratios =
                            normalize_split_ratios(child_ratios, grand_children.len());
                        for (grand_child, child_ratio) in
                            grand_children.into_iter().zip(child_ratios)
                        {
                            clean_children.push(grand_child);
                            clean_ratios.push(ratio * child_ratio);
                        }
                    }
                    child => {
                        clean_children.push(child);
                        clean_ratios.push(ratio);
                    }
                }
            }
            split_from_children(axis, clean_ratios, clean_children)
        }
    }
}

fn split_from_children(
    axis: SplitAxis,
    ratios: Vec<f64>,
    children: Vec<TerminalSplitNode>,
) -> Option<TerminalSplitNode> {
    if children.is_empty() {
        return None;
    }
    if children.len() == 1 {
        return children.into_iter().next();
    }
    Some(TerminalSplitNode::Split {
        axis,
        ratios: normalize_split_ratios(ratios, children.len()),
        children,
    })
}

fn compatible_top_grid_from_split_tree(
    tree: &TerminalSplitNode,
    pane_count: usize,
) -> Option<TerminalTopGrid> {
    match tree {
        TerminalSplitNode::Leaf { pane } => {
            (*pane == 0 && pane_count == 1).then(|| single_row_top_grid(vec![1.0], 1))
        }
        TerminalSplitNode::Split {
            axis: SplitAxis::Horizontal,
            ratios,
            children,
        } => {
            let column_ratios = normalize_split_ratios(ratios.clone(), children.len());
            let mut columns = Vec::new();
            let mut expected_pane = 0usize;
            for (child, ratio) in children.iter().zip(column_ratios) {
                let column = top_grid_column_from_split_child(child, ratio, &mut expected_pane)?;
                columns.push(column);
            }
            (expected_pane == pane_count).then_some(TerminalTopGrid { columns })
        }
        TerminalSplitNode::Split {
            axis: SplitAxis::Vertical,
            ratios,
            children,
        } => {
            let mut expected_pane = 0usize;
            let mut row_ratios = Vec::new();
            for (child, ratio) in children
                .iter()
                .zip(normalize_split_ratios(ratios.clone(), children.len()))
            {
                match child {
                    TerminalSplitNode::Leaf { pane } if *pane == expected_pane => {
                        expected_pane += 1;
                        row_ratios.push(ratio);
                    }
                    _ => return None,
                }
            }
            (expected_pane == pane_count).then_some(TerminalTopGrid {
                columns: vec![TerminalGridColumn {
                    ratio: 1.0,
                    rows: row_ratios.len(),
                    row_ratios,
                }],
            })
        }
    }
}

fn top_grid_column_from_split_child(
    child: &TerminalSplitNode,
    ratio: f64,
    expected_pane: &mut usize,
) -> Option<TerminalGridColumn> {
    match child {
        TerminalSplitNode::Leaf { pane } if *pane == *expected_pane => {
            *expected_pane += 1;
            Some(TerminalGridColumn {
                ratio,
                rows: 1,
                row_ratios: vec![1.0],
            })
        }
        TerminalSplitNode::Split {
            axis: SplitAxis::Vertical,
            ratios,
            children,
        } => {
            let mut row_ratios = Vec::new();
            for (child, row_ratio) in children
                .iter()
                .zip(normalize_split_ratios(ratios.clone(), children.len()))
            {
                match child {
                    TerminalSplitNode::Leaf { pane } if *pane == *expected_pane => {
                        *expected_pane += 1;
                        row_ratios.push(row_ratio);
                    }
                    _ => return None,
                }
            }
            Some(TerminalGridColumn {
                ratio,
                rows: row_ratios.len(),
                row_ratios,
            })
        }
        _ => None,
    }
}

pub fn normalize_split_ratios(ratios: Vec<f64>, count: usize) -> Vec<f64> {
    if count == 0 {
        return Vec::new();
    }
    let mut values = ratios
        .into_iter()
        .take(count)
        .map(|value| {
            if value.is_finite() {
                value.max(0.0)
            } else {
                0.0
            }
        })
        .collect::<Vec<_>>();
    while values.len() < count {
        values.push(1.0);
    }
    let total = values.iter().sum::<f64>();
    if total <= 0.0 {
        return vec![1.0 / count as f64; count];
    }
    let mut values = values
        .into_iter()
        .map(|value| value / total)
        .collect::<Vec<_>>();
    let min = TERMINAL_SPLIT_MIN_RATIO.min(1.0 / count as f64);
    if count as f64 * min >= 1.0 {
        return vec![1.0 / count as f64; count];
    }
    for _ in 0..2 {
        let fixed = values
            .iter()
            .filter(|value| **value < min)
            .map(|_| min)
            .sum::<f64>();
        if fixed == 0.0 {
            break;
        }
        let flexible_total = values.iter().filter(|value| **value >= min).sum::<f64>();
        if flexible_total <= 0.0 {
            return vec![1.0 / count as f64; count];
        }
        let remaining = (1.0 - fixed).max(0.0);
        values = values
            .into_iter()
            .map(|value| {
                if value < min {
                    min
                } else {
                    value / flexible_total * remaining
                }
            })
            .collect();
    }
    let total = values.iter().sum::<f64>();
    if total <= 0.0 {
        vec![1.0 / count as f64; count]
    } else {
        values.into_iter().map(|value| value / total).collect()
    }
}

fn normalize_ratios(ratios: Vec<f64>, count: usize) -> Vec<f64> {
    if count == 0 {
        return Vec::new();
    }
    let mut values = ratios
        .into_iter()
        .take(count)
        .map(|value| {
            if value.is_finite() {
                value.max(0.0)
            } else {
                0.0
            }
        })
        .collect::<Vec<_>>();
    while values.len() < count {
        values.push(1.0 / count as f64);
    }
    let total = values.iter().sum::<f64>();
    if total <= 0.0 {
        return vec![1.0 / count as f64; count];
    }
    values.into_iter().map(|value| value / total).collect()
}

fn clamp_ratio(value: f64, min: f64, max: f64, fallback: f64) -> f64 {
    if !value.is_finite() {
        return fallback;
    }
    value.clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_layout_serialization_keeps_resizable_dimensions() {
        let layout = TerminalLayoutSummary {
            active_terminal_id: "terminal-1".to_string(),
            top_panes: vec![TerminalPaneSummary {
                title: "Split".to_string(),
                terminal_id: "terminal-1".to_string(),
            }],
            tabs: Vec::new(),
            top_ratios: vec![1.0],
            top_grid: TerminalTopGrid::default(),
            split_tree: None,
            bottom_ratio: 0.72,
            ..Default::default()
        };

        let value = serde_json::to_value(&layout).expect("serialize layout");
        assert!(value.get("activeTerminalId").is_none());
        assert_eq!(value["topRatios"][0].as_f64(), Some(1.0));
        assert_eq!(value["bottomRatio"].as_f64(), Some(0.72));
    }

    #[test]
    fn save_from_gpui_rejects_empty_layout_without_overwriting_existing_layout() {
        let support_dir = std::env::temp_dir().join(format!(
            "codux-terminal-layout-empty-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&support_dir).expect("create support dir");
        let service = TerminalLayoutService::new(support_dir.clone());

        service
            .save_from_gpui(
                "project-1::worktree-1",
                Vec::new(),
                vec![TerminalPaneSummary {
                    title: "Shell".to_string(),
                    terminal_id: "terminal-kept".to_string(),
                }],
                vec![1.0],
                0.24,
            )
            .expect("save initial layout");

        let error = service
            .save_from_gpui(
                "project-1::worktree-1",
                Vec::new(),
                Vec::new(),
                Vec::new(),
                0.24,
            )
            .expect_err("empty layout should be rejected");
        assert_eq!(error, "Terminal layout is empty.");

        let layout = service.load(Some("project-1::worktree-1"));
        assert_eq!(layout.active_terminal_id, "");
        assert_eq!(layout.top_panes.len(), 1);
        assert_eq!(layout.top_panes[0].terminal_id, "terminal-kept");

        let _ = std::fs::remove_dir_all(support_dir);
    }

    #[test]
    fn legacy_top_ratios_migrate_to_single_row_grid() {
        let layout = sanitize_terminal_layout(TerminalLayoutSummary {
            top_panes: vec![
                TerminalPaneSummary {
                    title: "One".to_string(),
                    terminal_id: "terminal-1".to_string(),
                },
                TerminalPaneSummary {
                    title: "Two".to_string(),
                    terminal_id: "terminal-2".to_string(),
                },
            ],
            top_ratios: vec![1.0, 2.0],
            tabs: Vec::new(),
            active_terminal_id: String::new(),
            top_grid: TerminalTopGrid::default(),
            split_tree: None,
            bottom_ratio: 0.24,
            ..Default::default()
        })
        .expect("layout should sanitize");

        assert_eq!(layout.top_ratios, vec![1.0 / 3.0, 2.0 / 3.0]);
        assert_eq!(layout.top_grid.columns.len(), 2);
        assert_eq!(layout.top_grid.columns[0].rows, 1);
        assert_eq!(layout.top_grid.columns[1].ratio, 2.0 / 3.0);
    }

    #[test]
    fn invalid_grid_rows_rebuild_from_top_ratios() {
        let layout = sanitize_terminal_layout(TerminalLayoutSummary {
            top_panes: vec![
                TerminalPaneSummary {
                    title: "One".to_string(),
                    terminal_id: "terminal-1".to_string(),
                },
                TerminalPaneSummary {
                    title: "Two".to_string(),
                    terminal_id: "terminal-2".to_string(),
                },
            ],
            top_ratios: vec![0.25, 0.75],
            top_grid: TerminalTopGrid {
                columns: vec![TerminalGridColumn {
                    ratio: 1.0,
                    rows: 3,
                    row_ratios: vec![1.0, 1.0, 1.0],
                }],
            },
            split_tree: None,
            tabs: Vec::new(),
            active_terminal_id: String::new(),
            bottom_ratio: 0.24,
            ..Default::default()
        })
        .expect("layout should sanitize");

        assert_eq!(layout.top_grid.columns.len(), 2);
        assert_eq!(layout.top_grid.columns[0].ratio, 0.25);
        assert_eq!(layout.top_grid.columns[1].ratio, 0.75);
    }

    #[test]
    fn zero_row_grid_column_rebuilds_from_top_ratios() {
        let layout = sanitize_terminal_layout(TerminalLayoutSummary {
            top_panes: vec![
                TerminalPaneSummary {
                    title: "One".to_string(),
                    terminal_id: "terminal-1".to_string(),
                },
                TerminalPaneSummary {
                    title: "Two".to_string(),
                    terminal_id: "terminal-2".to_string(),
                },
            ],
            top_ratios: vec![0.2, 0.8],
            top_grid: TerminalTopGrid {
                columns: vec![
                    TerminalGridColumn {
                        ratio: 0.25,
                        rows: 0,
                        row_ratios: Vec::new(),
                    },
                    TerminalGridColumn {
                        ratio: 0.75,
                        rows: 2,
                        row_ratios: vec![0.5, 0.5],
                    },
                ],
            },
            split_tree: None,
            tabs: Vec::new(),
            active_terminal_id: String::new(),
            bottom_ratio: 0.24,
            ..Default::default()
        })
        .expect("layout should sanitize");

        assert_eq!(layout.top_grid.columns.len(), 2);
        assert_eq!(layout.top_grid.columns[0].rows, 1);
        assert_eq!(layout.top_grid.columns[0].ratio, 0.2);
        assert_eq!(layout.top_grid.columns[1].ratio, 0.8);
    }

    #[test]
    fn grid_roundtrip_serializes_columns() {
        let layout = TerminalLayoutSummary {
            top_panes: vec![
                TerminalPaneSummary {
                    title: "One".to_string(),
                    terminal_id: "terminal-1".to_string(),
                },
                TerminalPaneSummary {
                    title: "Two".to_string(),
                    terminal_id: "terminal-2".to_string(),
                },
            ],
            top_ratios: vec![0.5, 0.5],
            top_grid: TerminalTopGrid {
                columns: vec![TerminalGridColumn {
                    ratio: 1.0,
                    rows: 2,
                    row_ratios: vec![0.4, 0.6],
                }],
            },
            split_tree: None,
            tabs: Vec::new(),
            active_terminal_id: String::new(),
            bottom_ratio: 0.24,
            ..Default::default()
        };

        let json = serde_json::to_string(&layout).expect("serialize layout");
        let restored: TerminalLayoutSummary =
            serde_json::from_str(&json).expect("deserialize layout");
        assert_eq!(restored.top_grid.columns.len(), 1);
        assert_eq!(restored.top_grid.columns[0].rows, 2);
        assert_eq!(restored.top_grid.columns[0].row_ratios, vec![0.4, 0.6]);
    }
}
