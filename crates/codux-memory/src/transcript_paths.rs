use crate::{home_dir, normalized_string};
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
    let directory_name = project_path.replace(['/', '.'], "-");
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
        if matches!(row_type, Some("session_meta") | Some("turn_context"))
            && let Some(cwd) = payload.get("cwd").and_then(|value| value.as_str())
        {
            return paths_equivalent(Some(cwd), project_path);
        }
    }
    false
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
