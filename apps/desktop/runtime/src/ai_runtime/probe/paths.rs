use crate::{ai_runtime::state::normalized_string, runtime_paths::home_dir};
use serde_json::Value;
use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

pub(crate) fn find_codex_rollout_path(
    project_path: &str,
    external_session_id: &str,
) -> Option<PathBuf> {
    let sessions_dir = home_dir().join(".codex").join("sessions");
    recursive_files(&sessions_dir, "jsonl")
        .into_iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.contains(external_session_id))
                .unwrap_or(false)
                || codex_file_belongs_to_project(path, project_path)
        })
        .max_by_key(|path| file_modified_millis(path).unwrap_or(0))
}

pub(crate) fn claude_project_log_paths(project_path: &str) -> Vec<PathBuf> {
    let directory_name = project_path.replace('/', "-").replace('.', "-");
    let direct_dir = home_dir()
        .join(".claude")
        .join("projects")
        .join(directory_name);
    let direct = directory_files(&direct_dir, "jsonl");
    if !direct.is_empty() {
        return direct;
    }
    recursive_files(&home_dir().join(".claude").join("projects"), "jsonl")
        .into_iter()
        .filter(|path| {
            let Ok(file) = fs::File::open(path) else {
                return false;
            };
            let reader = BufReader::new(file);
            for line in reader.lines().map_while(Result::ok).take(12) {
                let Ok(row) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if let Some(cwd) = row.get("cwd").and_then(|value| value.as_str()) {
                    return paths_equivalent(Some(cwd), project_path);
                }
            }
            false
        })
        .collect()
}

pub(crate) fn gemini_session_paths(project_path: &str) -> Vec<PathBuf> {
    gemini_session_paths_for_roots(project_path, &[home_dir().join(".gemini")])
}

pub(crate) fn agy_session_paths(project_path: &str) -> Vec<PathBuf> {
    agy_session_paths_for_roots(
        project_path,
        &[home_dir().join(".gemini").join("antigravity-cli")],
    )
}

fn gemini_session_paths_for_roots(project_path: &str, roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for root_dir in roots {
        let temp_dir = root_dir.join("tmp");
        let projects_path = root_dir.join("projects.json");
        if let Ok(data) = fs::read(&projects_path) {
            if let Ok(root) = serde_json::from_slice::<Value>(&data) {
                if let Some(projects) = root.get("projects").and_then(|value| value.as_object()) {
                    for (stored_path, value) in projects {
                        if paths_equivalent(Some(stored_path), project_path) {
                            if let Some(directory) = value
                                .as_str()
                                .and_then(|value| normalized_string(Some(value)))
                            {
                                dirs.push(temp_dir.join(directory));
                            }
                        }
                    }
                }
            }
        }
        if let Ok(entries) = fs::read_dir(&temp_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let marker = path.join(".project_root");
                if let Ok(value) = fs::read_to_string(marker) {
                    if paths_equivalent(Some(value.trim()), project_path) {
                        dirs.push(path);
                    }
                }
            }
        }
    }
    let mut files = Vec::new();
    for dir in dirs {
        files.extend(directory_files(&dir.join("chats"), "json"));
    }
    files.retain(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.starts_with("session-"))
            .unwrap_or(false)
    });
    files.sort_by_key(|path| std::cmp::Reverse(file_modified_millis(path).unwrap_or(0)));
    files
}

fn agy_session_paths_for_roots(project_path: &str, roots: &[PathBuf]) -> Vec<PathBuf> {
    gemini_session_paths_for_roots(project_path, roots)
}

pub(super) fn find_kiro_session_path(
    project_path: &str,
    external_session_id: &str,
) -> Option<PathBuf> {
    let sessions_dir = home_dir().join(".kiro").join("sessions").join("cli");
    if !sessions_dir.exists() {
        return None;
    }
    let mut candidates = recursive_files(&sessions_dir, "json")
        .into_iter()
        .filter(|path| {
            path.file_stem()
                .and_then(|value| value.to_str())
                .map(|value| value.contains(external_session_id))
                .unwrap_or(false)
                || kiro_file_belongs_to_project(path, project_path)
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|path| std::cmp::Reverse(file_modified_millis(path).unwrap_or(0)));
    candidates.into_iter().next()
}

pub(super) fn directory_files(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some(extension))
        .collect::<Vec<_>>();
    files.sort();
    files
}

pub(super) fn recursive_files(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_recursive_files(dir, extension, &mut files);
    files.sort();
    files
}

pub(super) fn file_modified_millis(path: &Path) -> Option<u128> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis())
}

pub(crate) fn paths_equivalent(left: Option<&str>, right: &str) -> bool {
    let Some(left) = normalized_string(left) else {
        return false;
    };
    let Some(right) = normalized_string(Some(right)) else {
        return false;
    };
    let left = left.trim_end_matches('/');
    let right = right.trim_end_matches('/');
    left == right
}

fn codex_file_belongs_to_project(path: &Path, project_path: &str) -> bool {
    let Ok(file) = fs::File::open(path) else {
        return false;
    };
    let reader = BufReader::new(file);
    for line in reader.lines().map_while(Result::ok).take(20) {
        let Ok(row) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let row_type = row.get("type").and_then(|value| value.as_str());
        let payload = row.get("payload").unwrap_or(&Value::Null);
        if matches!(row_type, Some("session_meta") | Some("turn_context")) {
            if let Some(cwd) = payload.get("cwd").and_then(|value| value.as_str()) {
                return paths_equivalent(Some(cwd), project_path);
            }
        }
    }
    false
}

fn kiro_file_belongs_to_project(file_path: &Path, project_path: &str) -> bool {
    let Ok(data) = fs::read_to_string(file_path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<Value>(&data) else {
        return false;
    };
    [
        value.get("projectPath").and_then(|value| value.as_str()),
        value
            .get("project")
            .and_then(|value| value.get("path"))
            .and_then(|value| value.as_str()),
        value.get("cwd").and_then(|value| value.as_str()),
        value
            .get("workingDirectory")
            .and_then(|value| value.as_str()),
    ]
    .into_iter()
    .flatten()
    .any(|candidate| paths_equivalent(Some(candidate), project_path))
}

fn collect_recursive_files(dir: &Path, extension: &str, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_recursive_files(&path, extension, files);
        } else if path.extension().and_then(|value| value.to_str()) == Some(extension) {
            files.push(path);
        }
    }
}
