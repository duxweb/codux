use super::{AppSnapshot, ProjectRecord, ProjectSummary, ProjectWorktreeRecord};
use serde_json::Value;
use std::path::Path;
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
        runtime_target: project.runtime_target.clone(),
    }
}

pub(super) fn worktree_summary(
    worktree: &ProjectWorktreeRecord,
    runtime_target: &super::ProjectRuntimeTarget,
) -> ProjectSummary {
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
        runtime_target: runtime_target.clone(),
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
    let Some(segments) = badge_segments(name) else {
        return "PR".to_string();
    };

    let badge = badge_text_from_segments(&segments);
    if badge.is_empty() {
        "PR".to_string()
    } else {
        badge
    }
}

fn badge_text_from_segments(segments: &[String]) -> String {
    let chars = if segments.len() > 1
        && segments
            .iter()
            .all(|segment| segment.chars().all(|ch| ch.is_ascii_alphanumeric()))
    {
        segments
            .iter()
            .filter_map(|segment| segment.chars().next())
            .take(4)
            .collect::<Vec<_>>()
    } else {
        segments
            .iter()
            .flat_map(|segment| segment.chars())
            .take(4)
            .collect::<Vec<_>>()
    };

    chars.into_iter().collect::<String>().to_uppercase()
}

fn badge_segments(name: &str) -> Option<Vec<String>> {
    let mut segments = Vec::new();
    let mut current = String::new();
    let mut prev: Option<char> = None;

    for ch in name.chars() {
        if !ch.is_alphanumeric() {
            if !current.is_empty() {
                segments.push(std::mem::take(&mut current));
            }
            prev = None;
            continue;
        }

        if matches!(prev, Some(prev) if prev.is_lowercase() && ch.is_uppercase())
            && !current.is_empty()
        {
            segments.push(std::mem::take(&mut current));
        }

        current.push(ch);
        prev = Some(ch);
    }

    if !current.is_empty() {
        segments.push(current);
    }

    (!segments.is_empty()).then_some(segments)
}

pub(super) fn normalized_existing_path(path: &str) -> Result<String, String> {
    let normalized = codux_runtime_core::path::normalize_local_path(Path::new(path.trim()));
    if normalized.trim().is_empty() {
        return Err("Project path cannot be empty.".to_string());
    }
    if !Path::new(&normalized).exists() {
        return Err(format!("Project path does not exist: {normalized}"));
    }
    Ok(normalized)
}

/// Normalize a project path, validating **local** existence only for local
/// projects. A remote-hosted project's path lives on the host's filesystem
/// (e.g. a Windows `F:\test`), which neither exists on — nor can be
/// canonicalized to — this machine; for those we keep the host path verbatim
/// (just trimmed + non-empty). Without this, creating a remote project fails
/// the `Path::exists()` check and the editor silently refuses to save (the
/// error only lands in the main window's status bar).
pub(super) fn normalized_project_path(path: &str, is_hosted: bool) -> Result<String, String> {
    if !is_hosted {
        return normalized_existing_path(path);
    }
    let path = path.trim();
    if path.is_empty() {
        return Err("Project path cannot be empty.".to_string());
    }
    Ok(path.to_string())
}

pub(super) fn workspace_paths_equal(
    left: &str,
    right: &str,
    runtime_target: &super::ProjectRuntimeTarget,
) -> bool {
    runtime_target.paths_equal(left, right)
}

pub(super) fn normalized_project_name(name: &str, path: &str, is_hosted: bool) -> String {
    let trimmed = name.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    if is_hosted {
        codux_runtime_core::path::file_name(path).unwrap_or_else(|| "Project".to_string())
    } else {
        Path::new(path)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("Project")
            .to_string()
    }
}

pub(super) fn project_uuid(name: &str, path: &str, target_identity: Option<&str>) -> String {
    let identity = target_identity
        .map(|identity| format!(":{identity}"))
        .unwrap_or_default();
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("codux:project:{name}:{path}{identity}").as_bytes(),
    )
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badge_from_name_uses_word_initials_or_first_four_chars() {
        assert_eq!(badge_from_name("codux"), "CODU");
        assert_eq!(badge_from_name("codux-gpui"), "CG");
        assert_eq!(badge_from_name("Codux GPUI"), "CG");
        assert_eq!(badge_from_name("getUserInfo"), "GUI");
        assert_eq!(badge_from_name("wx-pay-api"), "WPA");
        assert_eq!(badge_from_name("a-b-c-d-e"), "ABCD");
        assert_eq!(badge_from_name("项目"), "项目");
        assert_eq!(badge_from_name("用户中心"), "用户中心");
        assert_eq!(badge_from_name("  "), "PR");
    }
}
