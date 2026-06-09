use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const TERMINAL_LAYOUT_NAMESPACE: &str = "terminal-layout";
const DEFAULT_BOTTOM_RATIO: f64 = 0.24;

pub fn terminal_layout_storage_key(project_id: &str, worktree_id: &str) -> String {
    format!("{project_id}::{worktree_id}")
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalLayoutSummary {
    #[serde(default, skip_serializing)]
    pub active_terminal_id: String,
    pub top_panes: Vec<TerminalPaneSummary>,
    pub tabs: Vec<TerminalTabSummary>,
    #[serde(default)]
    pub top_ratios: Vec<f64>,
    #[serde(default = "default_bottom_ratio")]
    pub bottom_ratio: f64,
    pub error: Option<String>,
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
        _active_terminal_id: String,
        top_panes: Vec<TerminalPaneSummary>,
        top_ratios: Vec<f64>,
        bottom_ratio: f64,
    ) -> Result<TerminalLayoutSummary, String> {
        if tabs.is_empty() && top_panes.is_empty() {
            return Err("Terminal layout is empty.".to_string());
        }
        let layout = TerminalLayoutSummary {
            tabs,
            active_terminal_id: String::new(),
            top_panes,
            top_ratios,
            bottom_ratio,
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
    if layout.tabs.is_empty() && layout.top_panes.is_empty() {
        return None;
    }
    layout.top_ratios = normalize_ratios(layout.top_ratios, layout.top_panes.len());
    layout.bottom_ratio = clamp_ratio(layout.bottom_ratio, 0.16, 0.58, default_bottom_ratio());
    Some(layout)
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
            bottom_ratio: 0.72,
            error: None,
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
                "terminal-kept".to_string(),
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
                String::new(),
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
}
