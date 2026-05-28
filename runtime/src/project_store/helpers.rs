use super::{AppSnapshot, ProjectRecord, ProjectSummary, ProjectWorktreeRecord};
use serde_json::Value;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub(super) fn project_summary(project: &ProjectRecord) -> ProjectSummary {
    ProjectSummary {
        id: project.id.clone(),
        name: project.name.clone(),
        path: project.path.clone(),
        badge: project
            .badge_text
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| badge_from_name(&project.name)),
        status: "active".to_string(),
        branch: "master".to_string(),
        changes: 0,
        badge_symbol: project.badge_symbol.clone(),
        badge_color_hex: project.badge_color_hex.clone(),
        git_default_push_remote_name: project.git_default_push_remote_name.clone(),
    }
}

pub(super) fn worktree_summary(worktree: &ProjectWorktreeRecord) -> ProjectSummary {
    ProjectSummary {
        id: worktree.id.clone(),
        name: worktree.name.clone(),
        path: worktree.path.clone(),
        badge: badge_from_name(&worktree.name),
        status: worktree.status.clone(),
        branch: worktree.branch.clone(),
        changes: 0,
        badge_symbol: None,
        badge_color_hex: None,
        git_default_push_remote_name: None,
    }
}

pub(super) fn is_known_workspace_id(snapshot: &AppSnapshot, project_id: &str) -> bool {
    snapshot
        .projects
        .iter()
        .any(|project| project.id == project_id)
        || snapshot
            .worktrees
            .iter()
            .any(|worktree| worktree.id == project_id)
}

pub(super) fn optional_string_value(value: Option<&str>) -> Value {
    normalized_string(value.unwrap_or_default())
        .map(Value::String)
        .unwrap_or(Value::Null)
}

pub(super) fn normalized_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

pub(crate) fn badge_from_name(name: &str) -> String {
    let letters = name
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter_map(|part| part.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase();
    if letters.is_empty() {
        "PR".to_string()
    } else {
        letters
    }
}

pub(super) fn normalized_existing_path(path: &str) -> Result<String, String> {
    let normalized = normalize_path(path);
    if normalized.trim().is_empty() {
        return Err("Project path cannot be empty.".to_string());
    }
    if !Path::new(&normalized).exists() {
        return Err(format!("Project path does not exist: {normalized}"));
    }
    Ok(normalized)
}

pub(super) fn normalize_path(path: &str) -> String {
    PathBuf::from(path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(path))
        .to_string_lossy()
        .trim()
        .to_string()
}

pub(super) fn normalized_project_name(name: &str, path: &str) -> String {
    let trimmed = name.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("Project")
        .to_string()
}

pub(super) fn project_uuid(name: &str, path: &str) -> String {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("codux:project:{name}:{path}").as_bytes(),
    )
    .to_string()
}
