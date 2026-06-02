use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const TERMINAL_LAYOUT_NAMESPACE: &str = "terminal-layout";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalLayoutSummary {
    pub active_slot_id: String,
    pub active_tab_id: String,
    pub top_panes: Vec<TerminalPaneSummary>,
    pub tabs: Vec<TerminalTabSummary>,
    pub top_ratios: Vec<f64>,
    pub bottom_ratio: f64,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalPaneSummary {
    pub id: String,
    pub title: String,
    pub terminal_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalTabSummary {
    pub id: String,
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
            return layout;
        }
        TerminalLayoutSummary {
            bottom_ratio: 0.32,
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
        let cache =
            crate::persistent_cache::PersistentCacheStore::for_support_dir(self.support_dir.clone())
                .ok();
        project_ids
            .into_iter()
            .filter_map(|project_id| {
                let layout = cache
                    .as_ref()
                    .and_then(|cache| {
                        cache
                            .get_json::<TerminalLayoutSummary>(
                                TERMINAL_LAYOUT_NAMESPACE,
                                project_id,
                            )
                            .ok()
                            .flatten()
                    })?;
                Some((project_id.to_string(), layout))
            })
            .collect()
    }

    pub fn save_from_gpui(
        &self,
        project_id: &str,
        tabs: Vec<TerminalTabSummary>,
        active_tab_id: String,
        top_panes: Vec<TerminalPaneSummary>,
        active_slot_id: String,
    ) -> Result<TerminalLayoutSummary, String> {
        let top_ratios = if top_panes.is_empty() {
            Vec::new()
        } else {
            vec![1.0 / top_panes.len() as f64; top_panes.len()]
        };
        let layout = TerminalLayoutSummary {
            tabs,
            active_tab_id,
            top_panes,
            top_ratios,
            bottom_ratio: 0.32,
            active_slot_id,
            error: None,
        };
        crate::persistent_cache::PersistentCacheStore::for_support_dir(self.support_dir.clone())?
            .put_json_debounced(TERMINAL_LAYOUT_NAMESPACE, project_id, &layout)?;
        Ok(layout)
    }
}

pub(crate) fn terminal_layout_cache_namespace() -> &'static str {
    TERMINAL_LAYOUT_NAMESPACE
}
