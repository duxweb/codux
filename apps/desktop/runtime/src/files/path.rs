use std::{
    fs,
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
};

pub(super) fn canonical_root(root_path: &str) -> Result<PathBuf, String> {
    let trimmed = root_path.trim();
    if trimmed.is_empty() {
        return Err("Project path is empty.".to_string());
    }
    let root = PathBuf::from(trimmed)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !root.is_dir() {
        return Err("Project path is not a directory.".to_string());
    }
    Ok(root)
}

pub(super) fn resolve_existing_path(root: &Path, raw_path: &str) -> Result<PathBuf, String> {
    let relative = sanitize_relative_path(raw_path)?;
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !path.starts_with(root) {
        return Err("Path escapes the project root.".to_string());
    }
    Ok(path)
}

pub(super) fn resolve_existing_project_entry(
    root: &Path,
    raw_path: &str,
) -> Result<PathBuf, String> {
    let relative = sanitize_relative_path(raw_path)?;
    let path = root.join(relative);
    ensure_within_root(root, &path)?;
    if !path.exists() {
        return Err("Path does not exist.".to_string());
    }
    Ok(path)
}

pub(super) fn resolve_new_child_path(
    root_path: &str,
    parent_path: Option<&str>,
    name: &str,
) -> Result<PathBuf, String> {
    let root = canonical_root(root_path)?;
    let parent = match parent_path.and_then(non_empty) {
        Some(path) => resolve_existing_path(&root, path)?,
        None => root.clone(),
    };
    if !parent.is_dir() {
        return Err("Target path is not a directory.".to_string());
    }
    let child = parent.join(clean_child_name(name)?);
    ensure_within_root(&root, &child)?;
    if child.exists() {
        return Err("A file with this name already exists.".to_string());
    }
    Ok(child)
}

pub(super) fn ensure_within_root(root: &Path, path: &Path) -> Result<(), String> {
    if path == root || path.starts_with(root) {
        return Ok(());
    }
    Err("Path is outside the current project.".to_string())
}

pub(super) fn clean_child_name(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.contains('/') || value.contains('\\') {
        return Err("Enter a valid file name.".to_string());
    }
    Ok(value.to_string())
}

fn sanitize_relative_path(raw_path: &str) -> Result<PathBuf, String> {
    let path = Path::new(raw_path.trim());
    if path.is_absolute() {
        return Err("Absolute paths are not allowed here.".to_string());
    }

    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            _ => return Err("Invalid relative path.".to_string()),
        }
    }
    Ok(clean)
}

pub(super) fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

pub(super) fn normalized_path_display(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub(super) fn normalized_path_key(path: &Path) -> String {
    normalized_path_display(path)
        .trim_end_matches('/')
        .to_lowercase()
}

pub(super) fn should_forward_file_watch_path(root_key: &str, path_key: &str) -> bool {
    if path_key.is_empty()
        || path_key.contains("/.git/")
        || path_key.ends_with("/.git")
        || path_key.contains("/node_modules/")
        || path_key.ends_with("/node_modules")
        || path_key.contains("/target/")
        || path_key.ends_with("/target")
    {
        return false;
    }
    path_key == root_key || path_key.starts_with(&format!("{root_key}/"))
}

pub(super) fn modified_at(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

pub(super) fn non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then_some(trimmed)
}

pub(super) fn should_hide_entry(name: &str) -> bool {
    matches!(name, ".git" | "node_modules" | "target")
}
