use crate::runtime_state::FileEntry;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

const FILE_TREE_STATE_NAMESPACE: &str = "file-tree-state";

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeStateSummary {
    #[serde(default)]
    pub files: Vec<FileEntry>,
    #[serde(default)]
    pub file_directory: String,
    #[serde(default)]
    pub selected_file_entry: Option<String>,
    #[serde(default)]
    pub selected_file_entries: Vec<String>,
    #[serde(default)]
    pub file_selection_anchor: Option<String>,
    #[serde(default)]
    pub file_tree_expanded_dirs: Vec<String>,
    #[serde(default)]
    pub file_tree_children: HashMap<String, Vec<FileEntry>>,
    #[serde(default)]
    pub error: Option<String>,
}

pub struct FileTreeStateService {
    support_dir: PathBuf,
}

impl FileTreeStateService {
    pub fn new(support_dir: PathBuf) -> Self {
        Self { support_dir }
    }

    pub fn load(&self, owner_id: Option<&str>) -> FileTreeStateSummary {
        let Some(owner_id) = owner_id else {
            return FileTreeStateSummary {
                error: Some("No selected project workspace.".to_string()),
                ..Default::default()
            };
        };
        self.cache_state(owner_id).unwrap_or_default()
    }

    pub fn load_many<'a, I>(&self, owner_ids: I) -> HashMap<String, FileTreeStateSummary>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let cache =
            crate::persistent_cache::PersistentCacheStore::for_support_dir(self.support_dir.clone())
                .ok();
        owner_ids
            .into_iter()
            .map(|owner_id| {
                let state = cache
                    .as_ref()
                    .and_then(|cache| {
                        cache
                            .get_json::<FileTreeStateSummary>(FILE_TREE_STATE_NAMESPACE, owner_id)
                            .ok()
                            .flatten()
                    })
                    .unwrap_or_default();
                (owner_id.to_string(), state)
            })
            .collect()
    }

    pub fn save(&self, owner_id: &str, state: &FileTreeStateSummary) -> Result<(), String> {
        crate::persistent_cache::PersistentCacheStore::for_support_dir(self.support_dir.clone())?
            .put_json_debounced(FILE_TREE_STATE_NAMESPACE, owner_id, state)
    }

    pub fn delete(&self, owner_id: &str) -> Result<bool, String> {
        crate::persistent_cache::PersistentCacheStore::for_support_dir(self.support_dir.clone())?
            .delete_json(FILE_TREE_STATE_NAMESPACE, owner_id)
    }

    fn cache_state(&self, owner_id: &str) -> Option<FileTreeStateSummary> {
        crate::persistent_cache::PersistentCacheStore::for_support_dir(self.support_dir.clone())
            .ok()?
            .get_json::<FileTreeStateSummary>(FILE_TREE_STATE_NAMESPACE, owner_id)
            .ok()
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        thread,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn saves_and_loads_file_tree_state() {
        let support_dir = temp_support_dir("file-tree-state");
        let service = FileTreeStateService::new(support_dir.clone());
        service
            .save(
                "worktree-1",
                &FileTreeStateSummary {
                    files: vec![FileEntry {
                        name: "src".to_string(),
                        relative_path: "src".to_string(),
                        kind: crate::runtime_state::FileKind::Directory,
                        size: 0,
                    }],
                    file_directory: "src".to_string(),
                    selected_file_entry: Some("src/main.rs".to_string()),
                    selected_file_entries: vec!["src/main.rs".to_string()],
                    file_selection_anchor: Some("src/main.rs".to_string()),
                    file_tree_expanded_dirs: vec!["src".to_string()],
                    file_tree_children: HashMap::from([(
                        "src".to_string(),
                        vec![FileEntry {
                            name: "main.rs".to_string(),
                            relative_path: "src/main.rs".to_string(),
                            kind: crate::runtime_state::FileKind::File,
                            size: 12,
                        }],
                    )]),
                    error: None,
                },
            )
            .expect("save file tree state");

        let loaded = wait_for_state(&service, "worktree-1");
        assert_eq!(loaded.file_directory, "src");
        assert_eq!(loaded.selected_file_entry.as_deref(), Some("src/main.rs"));
        assert_eq!(loaded.file_tree_expanded_dirs, vec!["src"]);
        assert_eq!(loaded.file_tree_children["src"][0].name, "main.rs");

        std::fs::remove_dir_all(support_dir).ok();
    }

    fn wait_for_state(service: &FileTreeStateService, owner_id: &str) -> FileTreeStateSummary {
        let started = Instant::now();
        loop {
            let state = service.load(Some(owner_id));
            if !state.files.is_empty() || started.elapsed() > Duration::from_secs(2) {
                return state;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn temp_support_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("codux-{label}-{nanos}"))
    }
}
