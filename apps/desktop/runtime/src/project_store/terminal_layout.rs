use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalLayoutRecord {
    #[serde(default)]
    pub tabs: Vec<TerminalBottomTabRecord>,
    #[serde(default, skip_serializing)]
    pub active_terminal_id: String,
    #[serde(default)]
    pub top_panes: Vec<TerminalTopPaneRecord>,
    #[serde(default, skip_serializing)]
    pub top_ratios: Vec<f64>,
    #[serde(default = "default_bottom_ratio", skip_serializing)]
    pub bottom_ratio: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalBottomTabRecord {
    pub label: String,
    pub terminal_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalTopPaneRecord {
    pub title: String,
    pub terminal_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalLayoutsSnapshot {
    pub layouts: HashMap<String, TerminalLayoutRecord>,
}

pub(super) fn sanitize_terminal_layout(
    layout: TerminalLayoutRecord,
) -> Option<TerminalLayoutRecord> {
    let tabs = sanitize_bottom_tabs(layout.tabs);
    let (top_panes, top_ratios) =
        sanitize_top_pane_ratio_entries(layout.top_panes, layout.top_ratios);
    if tabs.is_empty() && top_panes.is_empty() {
        return None;
    }
    Some(TerminalLayoutRecord {
        tabs,
        active_terminal_id: String::new(),
        top_panes,
        top_ratios,
        bottom_ratio: clamp_ratio(layout.bottom_ratio, 0.18, 0.72, default_bottom_ratio()),
    })
}

fn sanitize_bottom_tabs(tabs: Vec<TerminalBottomTabRecord>) -> Vec<TerminalBottomTabRecord> {
    let mut seen = HashSet::new();
    tabs.into_iter()
        .filter_map(|tab| {
            let terminal_id = normalized_string(&tab.terminal_id)?;
            if !seen.insert(terminal_id.clone()) {
                return None;
            }
            Some(TerminalBottomTabRecord {
                label: normalized_string(&tab.label).unwrap_or_else(|| "Tab".to_string()),
                terminal_id,
            })
        })
        .collect::<Vec<_>>()
}

fn sanitize_top_pane_ratio_entries(
    panes: Vec<TerminalTopPaneRecord>,
    ratios: Vec<f64>,
) -> (Vec<TerminalTopPaneRecord>, Vec<f64>) {
    let mut seen = HashSet::new();
    let next = panes
        .into_iter()
        .enumerate()
        .filter_map(|(index, pane)| {
            let terminal_id = normalized_string(&pane.terminal_id)?;
            if !seen.insert(terminal_id.clone()) {
                return None;
            }
            Some((
                TerminalTopPaneRecord {
                    title: normalized_string(&pane.title).unwrap_or_else(|| "Split".to_string()),
                    terminal_id,
                },
                ratios.get(index).copied().unwrap_or(0.0),
            ))
        })
        .collect::<Vec<_>>();
    let top_panes = next
        .iter()
        .map(|(pane, _)| pane.clone())
        .collect::<Vec<_>>();
    let top_ratios = normalize_ratios(
        next.into_iter().map(|(_, ratio)| ratio).collect(),
        top_panes.len(),
    );
    (top_panes, top_ratios)
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

fn default_bottom_ratio() -> f64 {
    0.32
}

fn normalized_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
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
    fn sanitize_terminal_layout_drops_empty_records_and_keeps_array_order() {
        let layout = sanitize_terminal_layout(TerminalLayoutRecord {
            tabs: vec![
                TerminalBottomTabRecord {
                    label: "  Second  ".to_string(),
                    terminal_id: "term-2".to_string(),
                },
                TerminalBottomTabRecord {
                    label: String::new(),
                    terminal_id: "term-1".to_string(),
                },
                TerminalBottomTabRecord {
                    label: "Duplicate".to_string(),
                    terminal_id: "term-1".to_string(),
                },
            ],
            active_terminal_id: "missing".to_string(),
            top_panes: vec![TerminalTopPaneRecord {
                title: String::new(),
                terminal_id: "term-3".to_string(),
            }],
            top_ratios: vec![0.0],
            bottom_ratio: 0.99,
        })
        .unwrap();

        assert_eq!(layout.tabs.len(), 2);
        assert_eq!(layout.tabs[0].terminal_id, "term-2");
        assert_eq!(layout.tabs[1].label, "Tab");
        assert_eq!(layout.active_terminal_id, "");
        assert_eq!(layout.top_panes[0].title, "Split");
        assert_eq!(layout.top_ratios, vec![1.0]);
        assert_eq!(layout.bottom_ratio, 0.72);
    }

    #[test]
    fn sanitize_terminal_layout_rejects_empty_layout() {
        assert!(
            sanitize_terminal_layout(TerminalLayoutRecord {
                tabs: Vec::new(),
                active_terminal_id: String::new(),
                top_panes: Vec::new(),
                top_ratios: Vec::new(),
                bottom_ratio: 0.32,
            })
            .is_none()
        );
    }

    #[test]
    fn terminal_layout_record_serialization_omits_runtime_ui_state() {
        let layout = TerminalLayoutRecord {
            tabs: Vec::new(),
            active_terminal_id: "terminal-1".to_string(),
            top_panes: vec![TerminalTopPaneRecord {
                title: "Split".to_string(),
                terminal_id: "terminal-1".to_string(),
            }],
            top_ratios: vec![1.0],
            bottom_ratio: 0.72,
        };

        let value = serde_json::to_value(&layout).expect("serialize layout");
        assert!(value.get("activeTerminalId").is_none());
        assert!(value.get("topRatios").is_none());
        assert!(value.get("bottomRatio").is_none());
    }
}
