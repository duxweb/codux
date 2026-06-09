use super::{
    FileChangeEvent, FileWatchRegistration, canonical_root, normalized_path_display,
    normalized_path_key, should_forward_file_watch_path,
};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::{collections::HashMap, path::Path, sync::Mutex};

pub struct FileWatchManager {
    watchers: Mutex<HashMap<String, FileProjectWatcher>>,
}

struct FileProjectWatcher {
    _watcher: RecommendedWatcher,
    _project_path: String,
    ref_count: usize,
}

impl Default for FileWatchManager {
    fn default() -> Self {
        Self {
            watchers: Mutex::new(HashMap::new()),
        }
    }
}

impl FileWatchManager {
    pub fn registration(&self, project_path: &str) -> Result<FileWatchRegistration, String> {
        let root = canonical_root(project_path)?;
        Ok(FileWatchRegistration {
            project_path: normalized_path_display(&root),
        })
    }

    pub fn watch(
        &self,
        project_path: String,
        on_change: impl Fn(FileChangeEvent) + Send + 'static,
    ) -> Result<FileWatchRegistration, String> {
        let root = canonical_root(&project_path)?;
        let root_key = normalized_path_key(&root);
        let normalized_project_path = normalized_path_display(&root);
        let registration = FileWatchRegistration {
            project_path: normalized_project_path.clone(),
        };

        let mut watchers = self
            .watchers
            .lock()
            .map_err(|_| "File watcher lock is poisoned.".to_string())?;
        if watchers.contains_key(&root_key) {
            if let Some(existing) = watchers.get_mut(&root_key) {
                existing.ref_count = existing.ref_count.saturating_add(1);
            }
            return Ok(registration);
        }

        let root_key_for_event = root_key.clone();
        let project_path_for_event = normalized_project_path.clone();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            let Ok(event) = event else {
                return;
            };
            let changed_paths = event
                .paths
                .iter()
                .filter_map(|path| {
                    let key = normalized_path_key(path);
                    should_forward_file_watch_path(&root_key_for_event, &key)
                        .then(|| normalized_path_display(path))
                })
                .collect::<Vec<_>>();
            if changed_paths.is_empty() {
                return;
            }
            on_change(FileChangeEvent {
                project_path: project_path_for_event.clone(),
                changed_paths,
            });
        })
        .map_err(|error| error.to_string())?;

        watcher
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|error| error.to_string())?;
        watchers.insert(
            root_key,
            FileProjectWatcher {
                _watcher: watcher,
                _project_path: normalized_project_path,
                ref_count: 1,
            },
        );
        Ok(registration)
    }

    pub fn unwatch(&self, project_path: String) -> Result<(), String> {
        let key = canonical_root(&project_path)
            .map(|root| normalized_path_key(&root))
            .unwrap_or_else(|_| normalized_path_key(Path::new(project_path.trim())));
        let mut watchers = self
            .watchers
            .lock()
            .map_err(|_| "File watcher lock is poisoned.".to_string())?;
        if let Some(existing) = watchers.get_mut(&key) {
            if existing.ref_count > 1 {
                existing.ref_count -= 1;
                return Ok(());
            }
        }
        watchers.remove(&key);
        Ok(())
    }
}
