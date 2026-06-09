use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const FILE_EDITOR_LAYOUT_NAMESPACE: &str = "file-editor-layout";

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileEditorLayoutSummary {
    #[serde(default)]
    pub tabs: Vec<FileEditorTabSummary>,
    #[serde(default)]
    pub active_path: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileEditorTabSummary {
    pub path: String,
    pub label: String,
    #[serde(default)]
    pub language: String,
}

pub struct FileEditorLayoutService {
    support_dir: PathBuf,
}

impl FileEditorLayoutService {
    pub fn new(support_dir: PathBuf) -> Self {
        Self { support_dir }
    }

    pub fn load(&self, owner_id: Option<&str>) -> FileEditorLayoutSummary {
        let Some(owner_id) = owner_id else {
            return FileEditorLayoutSummary {
                error: Some("No selected project workspace.".to_string()),
                ..Default::default()
            };
        };
        if let Some(layout) = self.cache_layout(owner_id) {
            return layout;
        }
        FileEditorLayoutSummary::default()
    }

    fn cache_layout(&self, owner_id: &str) -> Option<FileEditorLayoutSummary> {
        crate::persistent_cache::PersistentCacheStore::for_support_dir(self.support_dir.clone())
            .ok()?
            .get_json::<FileEditorLayoutSummary>(FILE_EDITOR_LAYOUT_NAMESPACE, owner_id)
            .ok()
            .flatten()
    }

    pub fn load_many<'a, I>(
        &self,
        owner_ids: I,
    ) -> std::collections::HashMap<String, FileEditorLayoutSummary>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let cache = crate::persistent_cache::PersistentCacheStore::for_support_dir(
            self.support_dir.clone(),
        )
        .ok();
        owner_ids
            .into_iter()
            .map(|owner_id| {
                let layout = cache
                    .as_ref()
                    .and_then(|cache| {
                        cache
                            .get_json::<FileEditorLayoutSummary>(
                                FILE_EDITOR_LAYOUT_NAMESPACE,
                                owner_id,
                            )
                            .ok()
                            .flatten()
                    })
                    .unwrap_or_default();
                (owner_id.to_string(), layout)
            })
            .collect()
    }

    pub fn save_from_gpui(
        &self,
        owner_id: &str,
        tabs: Vec<FileEditorTabSummary>,
        active_path: Option<String>,
    ) -> Result<FileEditorLayoutSummary, String> {
        let cache = crate::persistent_cache::PersistentCacheStore::for_support_dir(
            self.support_dir.clone(),
        )?;
        if tabs.is_empty() {
            cache.delete_json(FILE_EDITOR_LAYOUT_NAMESPACE, owner_id)?;
        } else {
            let active_path = active_path
                .filter(|active| tabs.iter().any(|tab| tab.path == *active))
                .or_else(|| tabs.first().map(|tab| tab.path.clone()));
            let layout = FileEditorLayoutSummary {
                tabs,
                active_path,
                error: None,
            };
            cache.put_json(FILE_EDITOR_LAYOUT_NAMESPACE, owner_id, &layout)?;
            return Ok(layout);
        }
        Ok(self.load(Some(owner_id)))
    }

    pub fn delete(&self, owner_id: &str) -> Result<bool, String> {
        crate::persistent_cache::PersistentCacheStore::for_support_dir(self.support_dir.clone())?
            .delete_json(FILE_EDITOR_LAYOUT_NAMESPACE, owner_id)
    }
}
