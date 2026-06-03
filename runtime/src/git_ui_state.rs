use crate::{
    git::{GitFileStatus, GitReviewContentSummary, GitReviewSummary, GitSummary},
    persistent_cache::PersistentCacheStore,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

const GIT_UI_STATE_NAMESPACE: &str = "git-ui-state";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitUiStateSummary {
    #[serde(default)]
    pub git: GitSummary,
    #[serde(default)]
    pub git_review: GitReviewSummary,
    #[serde(default)]
    pub selected_git_file: Option<String>,
    #[serde(default)]
    pub selected_git_files: Vec<String>,
    #[serde(default)]
    pub selected_git_branch: Option<String>,
    #[serde(default)]
    pub git_expanded_sections: Vec<String>,
    #[serde(default)]
    pub git_expanded_dirs: Vec<String>,
    #[serde(default)]
    pub git_tree_children: HashMap<String, Vec<GitFileStatus>>,
    #[serde(default)]
    pub git_diff_preview: String,
    #[serde(default)]
    pub git_review_content: Option<GitReviewContentSummary>,
    #[serde(default)]
    pub error: Option<String>,
}

pub struct GitUiStateService {
    support_dir: PathBuf,
}

impl GitUiStateService {
    pub fn new(support_dir: PathBuf) -> Self {
        Self { support_dir }
    }

    pub fn load(&self, owner_id: Option<&str>) -> GitUiStateSummary {
        let Some(owner_id) = owner_id else {
            return GitUiStateSummary {
                error: Some("No selected project workspace.".to_string()),
                ..Default::default()
            };
        };
        self.cache_state(owner_id).unwrap_or_default()
    }

    pub fn load_many<'a, I>(&self, owner_ids: I) -> HashMap<String, GitUiStateSummary>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let cache = PersistentCacheStore::for_support_dir(self.support_dir.clone()).ok();
        owner_ids
            .into_iter()
            .map(|owner_id| {
                let state = cache
                    .as_ref()
                    .and_then(|cache| {
                        cache
                            .get_json::<GitUiStateSummary>(GIT_UI_STATE_NAMESPACE, owner_id)
                            .ok()
                            .flatten()
                    })
                    .unwrap_or_default();
                (owner_id.to_string(), state)
            })
            .collect()
    }

    pub fn save(&self, owner_id: &str, state: &GitUiStateSummary) -> Result<(), String> {
        PersistentCacheStore::for_support_dir(self.support_dir.clone())?
            .put_json_debounced(GIT_UI_STATE_NAMESPACE, owner_id, state)
    }

    pub fn delete(&self, owner_id: &str) -> Result<bool, String> {
        PersistentCacheStore::for_support_dir(self.support_dir.clone())?
            .delete_json(GIT_UI_STATE_NAMESPACE, owner_id)
    }

    fn cache_state(&self, owner_id: &str) -> Option<GitUiStateSummary> {
        PersistentCacheStore::for_support_dir(self.support_dir.clone())
            .ok()?
            .get_json::<GitUiStateSummary>(GIT_UI_STATE_NAMESPACE, owner_id)
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
    fn saves_and_loads_git_ui_state() {
        let support_dir = temp_support_dir("git-ui-state");
        let service = GitUiStateService::new(support_dir.clone());
        service
            .save(
                "worktree-1",
                &GitUiStateSummary {
                    selected_git_file: Some("src/main.rs".to_string()),
                    selected_git_files: vec!["src/main.rs".to_string()],
                    selected_git_branch: Some("main".to_string()),
                    git_expanded_sections: vec!["changed".to_string()],
                    git_expanded_dirs: vec!["review\0src".to_string()],
                    git_tree_children: HashMap::from([(
                        "changed\0src".to_string(),
                        vec![GitFileStatus {
                            path: "src/main.rs".to_string(),
                            index_status: "M".to_string(),
                            worktree_status: "M".to_string(),
                        }],
                    )]),
                    ..Default::default()
                },
            )
            .expect("save git ui state");

        let loaded = wait_for_state(&service, "worktree-1");
        assert_eq!(loaded.selected_git_file.as_deref(), Some("src/main.rs"));
        assert_eq!(loaded.selected_git_branch.as_deref(), Some("main"));
        assert_eq!(loaded.git_expanded_sections, vec!["changed"]);
        assert_eq!(loaded.git_tree_children["changed\0src"][0].path, "src/main.rs");

        std::fs::remove_dir_all(support_dir).ok();
    }

    fn wait_for_state(service: &GitUiStateService, owner_id: &str) -> GitUiStateSummary {
        let started = Instant::now();
        loop {
            let state = service.load(Some(owner_id));
            if state.selected_git_file.is_some() || started.elapsed() > Duration::from_secs(2) {
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
