//! A tiny project store for the headless agent: a JSON list of
//! `{id, name, path}` the host serves via `project.list/add/remove`. The desktop
//! has a richer ProjectStore (worktrees, badges, …); the agent only needs the
//! list so a controller can pick a project to run terminals/files against.

use codux_runtime_core::project::ProjectListItem;
use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

/// The agent's persistent data directory (projects list, AI usage cache, …).
/// `CODUX_AGENT_DATA_DIR` overrides the default `~/.codux-agent`.
pub fn agent_data_dir() -> PathBuf {
    std::env::var("CODUX_AGENT_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .unwrap_or_else(|_| ".".to_string());
            PathBuf::from(home).join(".codux-agent")
        })
}

pub struct AgentProjectStore {
    path: PathBuf,
}

impl AgentProjectStore {
    pub fn new() -> Self {
        Self {
            path: agent_data_dir().join("projects.json"),
        }
    }

    pub fn list(&self) -> Vec<ProjectListItem> {
        fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| serde_json::from_str::<Vec<ProjectListItem>>(&text).ok())
            .unwrap_or_default()
    }

    pub fn add(&self, path: &str, name: Option<&str>) -> Result<Vec<ProjectListItem>, String> {
        let raw_path = path.trim();
        if raw_path.is_empty() {
            return Err("Project path is required.".to_string());
        }
        let path = raw_path.to_string();
        let name = name
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| default_project_name(&path));
        let mut items = self.list();
        if let Some(existing) = items.iter_mut().find(|item| {
            codux_runtime_core::path::local_paths_equal(Path::new(&item.path), Path::new(&path))
        }) {
            existing.name = name;
            existing.path = path;
        } else {
            items.push(ProjectListItem {
                id: project_id_for_path(&path),
                name,
                path,
            });
        }
        self.save(&items)?;
        Ok(items)
    }

    pub fn remove(&self, id: &str) -> Result<Vec<ProjectListItem>, String> {
        let mut items = self.list();
        items.retain(|item| item.id != id);
        self.save(&items)?;
        Ok(items)
    }

    fn save(&self, items: &[ProjectListItem]) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let text = serde_json::to_string_pretty(items).map_err(|error| error.to_string())?;
        fs::write(&self.path, text).map_err(|error| error.to_string())
    }
}

fn project_id_for_path(path: &str) -> String {
    let mut hasher = DefaultHasher::new();
    codux_runtime_core::path::local_path_identity_key(Path::new(path))
        .unwrap_or_default()
        .hash(&mut hasher);
    format!("p-{:016x}", hasher.finish())
}

fn default_project_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("Project")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_ids_and_names_follow_target_path_syntax() {
        #[cfg(windows)]
        assert_eq!(
            project_id_for_path(r"C:\Users\Dux\Project"),
            project_id_for_path("c:/users/dux/project/")
        );
        #[cfg(windows)]
        assert_eq!(default_project_name(r"C:\Users\Dux\Project"), "Project");
        #[cfg(not(windows))]
        assert_eq!(default_project_name("/home/dux/Project"), "Project");
        #[cfg(not(windows))]
        assert_ne!(
            project_id_for_path("/repo/Project"),
            project_id_for_path("/repo/project")
        );
    }
}
