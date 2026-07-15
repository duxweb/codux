use crate::{
    ai_runtime::{constants::CODEX_LIVE_TRANSCRIPT_TAIL_BYTES, state::normalized_string},
    runtime_paths::home_dir,
};
use serde_json::Value;
use std::{
    fs,
    io::{BufRead, BufReader, Seek},
    path::{Path, PathBuf},
};

pub(crate) fn find_codex_rollout_path(
    project_path: &str,
    external_session_id: &str,
) -> Option<PathBuf> {
    find_codex_rollout_path_since(project_path, external_session_id, None)
}

pub(crate) fn codex_session_id_from_rollout(path: &Path) -> Option<String> {
    codex_session_id_from_rollout_name(path).or_else(|| codex_session_id_from_session_meta(path))
}

pub(crate) fn find_codex_rollout_path_since(
    project_path: &str,
    external_session_id: &str,
    started_at: Option<f64>,
) -> Option<PathBuf> {
    let sessions_dir = home_dir().join(".codex").join("sessions");
    let files = recursive_files(&sessions_dir, "jsonl");
    let exact = files
        .iter()
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.contains(external_session_id))
                .unwrap_or(false)
        })
        .max_by_key(|path| file_modified_millis(path).unwrap_or(0))
        .cloned();
    if exact.is_some() {
        return exact;
    }
    files
        .into_iter()
        .filter(|path| file_modified_after_start(path, started_at))
        .filter(|path| codex_file_belongs_to_project_since(path, project_path, started_at))
        .max_by_key(|path| file_modified_millis(path).unwrap_or(0))
}

/// Freshest codex rollout whose recorded cwd matches the project and was touched
/// after this wrapper launch. This is the safe fallback when no external session
/// id is known; without the launch lower bound a newly opened terminal can bind
/// to a historical rollout and inherit its cumulative usage.
pub(crate) fn find_codex_rollout_by_cwd_since(
    project_path: &str,
    started_at: Option<f64>,
) -> Option<PathBuf> {
    let sessions_dir = home_dir().join(".codex").join("sessions");
    recursive_files(&sessions_dir, "jsonl")
        .into_iter()
        .filter(|path| file_modified_after_start(path, started_at))
        .filter(|path| codex_file_belongs_to_project_since(path, project_path, started_at))
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
            let mut reader = BufReader::new(file);
            let mut line = String::new();
            for _ in 0..12 {
                line.clear();
                let Ok(bytes) = reader.read_line(&mut line) else {
                    break;
                };
                if bytes == 0 {
                    break;
                }
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

/// Resolve a Kimi session root for a project. Current Kimi Code writes
/// `session_index.jsonl` entries pointing at
/// `<share>/sessions/wd_<name>_<hash>/session_<uuid>/`, with the main wire under
/// `agents/main/wire.jsonl` and `state.json` in the session root.
pub(crate) fn kimi_session_dir_since(
    project_path: &str,
    external_session_id: Option<&str>,
    started_at: Option<f64>,
) -> Option<PathBuf> {
    for share in kimi_share_dirs() {
        if !share.join("session_index.jsonl").exists() {
            continue;
        }
        if let Some(dir) =
            kimi_session_from_index(&share, external_session_id, Some(project_path), started_at)
        {
            return Some(dir);
        }
    }
    None
}

fn kimi_share_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(custom) = std::env::var("KIMI_SHARE_DIR") {
        let custom = custom.trim();
        if !custom.is_empty() {
            dirs.push(PathBuf::from(custom));
        }
    }
    dirs.push(home_dir().join(".kimi-code"));
    dirs
}

pub(crate) fn kimi_wire_path(session_dir: &Path) -> PathBuf {
    let direct = session_dir.join("wire.jsonl");
    if direct.exists() {
        return direct;
    }
    let current = session_dir.join("agents").join("main").join("wire.jsonl");
    if current.exists() {
        return current;
    }
    current
}

pub(crate) fn kimi_state_path(session_dir: &Path) -> PathBuf {
    let direct = session_dir.join("state.json");
    if direct.exists() {
        return direct;
    }
    session_dir
        .parent()
        .and_then(|path| path.parent())
        .map(|path| path.join("state.json"))
        .unwrap_or(direct)
}

fn kimi_session_from_index(
    share: &Path,
    external_session_id: Option<&str>,
    project_path: Option<&str>,
    started_at: Option<f64>,
) -> Option<PathBuf> {
    let file = fs::File::open(share.join("session_index.jsonl")).ok()?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    let mut matches = Vec::new();
    loop {
        line.clear();
        let Ok(bytes) = reader.read_line(&mut line) else {
            break;
        };
        if bytes == 0 {
            break;
        }
        let Ok(row) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let session_id = row
            .get("sessionId")
            .or_else(|| row.get("session_id"))
            .and_then(|value| value.as_str())
            .and_then(normalized_record_id);
        if let Some(expected) = external_session_id.and_then(normalized_record_id) {
            let id_matches = session_id == Some(expected)
                || row
                    .get("sessionDir")
                    .and_then(|value| value.as_str())
                    .and_then(|value| Path::new(value).file_name())
                    .and_then(|value| value.to_str())
                    == Some(expected);
            if !id_matches {
                continue;
            }
        }
        if let Some(project_path) = project_path {
            let work_dir = row
                .get("workDir")
                .or_else(|| row.get("workdir"))
                .or_else(|| row.get("cwd"))
                .and_then(|value| value.as_str());
            if !paths_equivalent(work_dir, project_path) {
                continue;
            }
        }
        let Some(session_dir) = row
            .get("sessionDir")
            .or_else(|| row.get("session_dir"))
            .and_then(|value| value.as_str())
            .and_then(|value| normalized_string(Some(value)))
            .map(PathBuf::from)
        else {
            continue;
        };
        let session_dir = if session_dir.is_absolute() {
            session_dir
        } else {
            share.join(session_dir)
        };
        if !kimi_wire_path(&session_dir).exists() {
            continue;
        }
        if !kimi_session_modified_after_start(&session_dir, started_at) {
            continue;
        }
        matches.push(session_dir);
    }
    matches
        .into_iter()
        .max_by_key(|path| kimi_session_modified_millis(path).unwrap_or(0))
}

fn kimi_session_modified_after_start(session_dir: &Path, started_at: Option<f64>) -> bool {
    file_modified_after_start(&kimi_wire_path(session_dir), started_at)
        || file_modified_after_start(&kimi_state_path(session_dir), started_at)
}

fn kimi_session_modified_millis(session_dir: &Path) -> Option<u128> {
    [
        file_modified_millis(&kimi_wire_path(session_dir)),
        file_modified_millis(&kimi_state_path(session_dir)),
    ]
    .into_iter()
    .flatten()
    .max()
}

pub(crate) fn kimi_runtime_resource_paths(
    project_path: Option<&str>,
    external_session_id: Option<&str>,
    started_at: Option<f64>,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for share in kimi_share_dirs() {
        push_unique_path(&mut paths, share.join("session_index.jsonl"));
    }
    if let Some(project_path) = normalized_string(project_path)
        && let Some(session_dir) =
            kimi_session_dir_since(&project_path, external_session_id, started_at)
    {
        push_unique_path(&mut paths, kimi_wire_path(&session_dir));
        push_unique_path(&mut paths, kimi_state_path(&session_dir));
    }
    paths
}

pub(crate) fn kiro_session_paths(
    project_path: &str,
    external_session_id: Option<&str>,
    started_at: Option<f64>,
) -> Option<(PathBuf, PathBuf)> {
    let sessions_dir = home_dir().join(".kiro").join("sessions").join("cli");
    if let Some(session_id) = external_session_id.and_then(normalized_record_id) {
        let json = sessions_dir.join(format!("{session_id}.json"));
        let jsonl = sessions_dir.join(format!("{session_id}.jsonl"));
        if json.exists() || jsonl.exists() {
            return Some((json, jsonl));
        }
    }
    directory_files(&sessions_dir, "json")
        .into_iter()
        .filter(|path| file_modified_after_start(path, started_at))
        .filter(|path| kiro_session_belongs_to_project(path, project_path))
        .max_by_key(|path| file_modified_millis(path).unwrap_or(0))
        .map(|json| {
            let jsonl = json.with_extension("jsonl");
            (json, jsonl)
        })
}

pub(crate) fn kiro_runtime_resource_paths(
    project_path: Option<&str>,
    external_session_id: Option<&str>,
    started_at: Option<f64>,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let sessions_dir = home_dir().join(".kiro").join("sessions").join("cli");
    if let Some(session_id) = external_session_id.and_then(normalized_record_id) {
        push_unique_path(&mut paths, sessions_dir.join(format!("{session_id}.json")));
        push_unique_path(&mut paths, sessions_dir.join(format!("{session_id}.jsonl")));
        return paths;
    }
    if let Some(project_path) = normalized_string(project_path) {
        if let Some((json, jsonl)) =
            kiro_session_paths(&project_path, external_session_id, started_at)
        {
            push_unique_path(&mut paths, json);
            push_unique_path(&mut paths, jsonl);
        } else {
            push_unique_path(&mut paths, sessions_dir);
        }
    }
    paths
}

pub(crate) fn opencode_database_paths() -> Vec<PathBuf> {
    let data_dir = xdg_data_dir().join("opencode");
    let mut paths = Vec::new();
    if let Some(value) = std::env::var_os("OPENCODE_DB").filter(|value| !value.is_empty()) {
        let path = PathBuf::from(value);
        push_unique_path(
            &mut paths,
            if path.is_absolute() {
                path
            } else {
                data_dir.join(path)
            },
        );
    }
    push_unique_path(&mut paths, data_dir.join("opencode.db"));
    push_unique_path(&mut paths, data_dir.join("opencode-local.db"));
    paths
}

pub(crate) fn opencode_runtime_resource_paths(terminal_id: &str) -> Vec<PathBuf> {
    database_runtime_resource_paths(opencode_database_paths(), terminal_id)
}

pub(crate) fn mimo_database_paths() -> Vec<PathBuf> {
    let data_dir = mimo_data_dir();
    let mut paths = Vec::new();
    if let Some(value) = std::env::var_os("MIMOCODE_DB").filter(|value| !value.is_empty()) {
        let path = PathBuf::from(value);
        push_unique_path(
            &mut paths,
            if path.is_absolute() {
                path
            } else {
                data_dir.join(path)
            },
        );
    }
    push_unique_path(&mut paths, data_dir.join("mimocode.db"));
    push_unique_path(&mut paths, data_dir.join("mimocode-local.db"));
    paths
}

fn mimo_data_dir() -> PathBuf {
    if let Some(home) = std::env::var_os("MIMOCODE_HOME").filter(|value| !value.is_empty()) {
        return PathBuf::from(home).join("data");
    }
    xdg_data_dir().join("mimocode")
}

pub(crate) fn mimo_runtime_resource_paths(terminal_id: &str) -> Vec<PathBuf> {
    database_runtime_resource_paths(mimo_database_paths(), terminal_id)
}

fn database_runtime_resource_paths(
    database_paths: Vec<PathBuf>,
    terminal_id: &str,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for database_path in database_paths {
        push_unique_path(&mut paths, database_path.clone());
        push_unique_path(
            &mut paths,
            PathBuf::from(format!("{}-wal", database_path.display())),
        );
        push_unique_path(
            &mut paths,
            PathBuf::from(format!("{}-shm", database_path.display())),
        );
    }
    if let Some(terminal_id) = normalized_record_id(terminal_id) {
        paths.push(
            crate::runtime_paths::opencode_session_map_dir()
                .join(format!("opencode-session-{terminal_id}.json")),
        );
    }
    paths
}

pub(crate) fn agy_conversation_db_for_runtime(
    project_path: &str,
    external_session_id: Option<&str>,
    started_at: Option<f64>,
) -> Option<PathBuf> {
    let dir = home_dir()
        .join(".gemini")
        .join("antigravity-cli")
        .join("conversations");
    agy_conversation_db_for_runtime_in(&dir, project_path, external_session_id, started_at)
}

fn agy_conversation_db_for_runtime_in(
    dir: &Path,
    project_path: &str,
    external_session_id: Option<&str>,
    started_at: Option<f64>,
) -> Option<PathBuf> {
    let files = directory_files(dir, "db");
    if let Some(session_id) = external_session_id.and_then(normalized_record_id) {
        return files
            .into_iter()
            .find(|path| path.file_stem().and_then(|name| name.to_str()) == Some(session_id))
            .filter(|path| agy_db_belongs_to_project(path, project_path));
    }
    files
        .into_iter()
        .filter(|path| agy_db_belongs_to_project_and_started_after(path, project_path, started_at))
        .max_by_key(|path| file_modified_millis(path).unwrap_or(0))
}

pub(crate) fn agy_runtime_resource_paths(
    project_path: Option<&str>,
    external_session_id: Option<&str>,
    started_at: Option<f64>,
) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Some(database_path) = normalized_string(project_path)
        .and_then(|path| agy_conversation_db_for_runtime(&path, external_session_id, started_at))
    {
        push_unique_path(&mut paths, database_path);
    }
    paths
}

fn normalized_record_id(id: &str) -> Option<&str> {
    let trimmed = id.trim();
    if trimmed.is_empty() || trimmed != id {
        return None;
    }
    trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
        .then_some(trimmed)
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

fn agy_db_belongs_to_project(path: &Path, project_path: &str) -> bool {
    codux_ai_history::agy_db::parse_agy_conversation_db(path)
        .and_then(|conversation| conversation.project_path)
        .map(|path| paths_equivalent(Some(&path), project_path))
        .unwrap_or(false)
}

fn agy_db_belongs_to_project_and_started_after(
    path: &Path,
    project_path: &str,
    started_at: Option<f64>,
) -> bool {
    let Some(started_at) = started_at else {
        return false;
    };
    let Some(conversation) = codux_ai_history::agy_db::parse_agy_conversation_db(path) else {
        return false;
    };
    if !paths_equivalent(conversation.project_path.as_deref(), project_path) {
        return false;
    }
    conversation
        .last_user_at
        .into_iter()
        .chain(conversation.last_model_at)
        .any(|timestamp| timestamp + 1.0 >= started_at)
}

fn kiro_session_belongs_to_project(path: &Path, project_path: &str) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    let Ok(root) = serde_json::from_slice::<Value>(&bytes) else {
        return false;
    };
    root.get("cwd")
        .and_then(|value| value.as_str())
        .map(|cwd| paths_equivalent(Some(cwd), project_path))
        .unwrap_or(false)
}

fn xdg_data_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home_dir().join(".local").join("share"))
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

pub(super) fn file_modified_after_start(path: &Path, started_at: Option<f64>) -> bool {
    let Some(started_at) = started_at else {
        return true;
    };
    let Some(modified_millis) = file_modified_millis(path) else {
        return false;
    };
    modified_millis as f64 + 1_000.0 >= started_at * 1000.0
}

pub(crate) use codux_runtime_core::path::optional_local_path_equals as paths_equivalent;

fn codex_file_belongs_to_project_since(
    path: &Path,
    project_path: &str,
    started_at: Option<f64>,
) -> bool {
    let Some(mut reader) = codex_project_match_reader(path, started_at) else {
        return false;
    };
    let mut line = String::new();
    loop {
        line.clear();
        let Ok(bytes) = reader.read_line(&mut line) else {
            break;
        };
        if bytes == 0 {
            break;
        }
        let Ok(row) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(started_at) = started_at {
            let row_timestamp = row
                .get("timestamp")
                .and_then(|value| value.as_str())
                .and_then(super::common::parse_iso8601_seconds)
                .or_else(|| {
                    row.get("payload")
                        .and_then(|payload| payload.get("timestamp"))
                        .and_then(|value| value.as_str())
                        .and_then(super::common::parse_iso8601_seconds)
                });
            if row_timestamp
                .map(|timestamp| timestamp + 1.0 < started_at)
                .unwrap_or(true)
            {
                continue;
            }
        }
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

fn codex_project_match_reader(path: &Path, started_at: Option<f64>) -> Option<BufReader<fs::File>> {
    let mut file = fs::File::open(path).ok()?;
    let metadata = file.metadata().ok()?;
    if started_at.is_some() && metadata.len() > CODEX_LIVE_TRANSCRIPT_TAIL_BYTES {
        let start = metadata
            .len()
            .saturating_sub(CODEX_LIVE_TRANSCRIPT_TAIL_BYTES);
        file.seek(std::io::SeekFrom::Start(start)).ok()?;
        let mut reader = BufReader::with_capacity(32 * 1024, file);
        if start > 0 {
            let mut partial = String::new();
            reader.read_line(&mut partial).ok()?;
        }
        return Some(reader);
    }
    Some(BufReader::new(file))
}

fn codex_session_id_from_rollout_name(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    if stem.len() < 36 {
        return None;
    }
    let candidate = &stem[stem.len() - 36..];
    uuid::Uuid::parse_str(candidate)
        .ok()
        .map(|_| candidate.to_string())
}

fn codex_session_id_from_session_meta(path: &Path) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    for _ in 0..20 {
        line.clear();
        let Ok(bytes) = reader.read_line(&mut line) else {
            break;
        };
        if bytes == 0 {
            break;
        }
        let Ok(row) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if row.get("type").and_then(|value| value.as_str()) != Some("session_meta") {
            continue;
        }
        let payload = row.get("payload").unwrap_or(&Value::Null);
        for key in ["session_id", "sessionId", "id"] {
            if let Some(session_id) = payload
                .get(key)
                .and_then(|value| value.as_str())
                .and_then(|value| normalized_string(Some(value)))
            {
                return Some(session_id);
            }
        }
    }
    None
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codex_session_id_from_rollout_uses_file_name() {
        let path = PathBuf::from(
            "/tmp/rollout-2026-06-28T10-43-07-019f0c1b-f835-7c33-a4f4-3e737d2fbf90.jsonl",
        );

        assert_eq!(
            codex_session_id_from_rollout(&path).as_deref(),
            Some("019f0c1b-f835-7c33-a4f4-3e737d2fbf90")
        );
    }

    #[test]
    fn runtime_probe_matches_windows_path_forms() {
        #[cfg(windows)]
        assert!(paths_equivalent(
            Some(r"\\?\C:\Users\Dux\project\"),
            "c:/users/dux/project"
        ));
        assert!(!paths_equivalent(
            Some(r"C:\Users\Dux\project-child"),
            r"C:\Users\Dux\project"
        ));
    }

    #[test]
    fn codex_session_id_from_rollout_falls_back_to_session_meta() {
        let dir =
            std::env::temp_dir().join(format!("codux-codex-rollout-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rollout.jsonl");
        fs::write(
            &path,
            r#"{"timestamp":"2026-06-28T00:00:00Z","type":"session_meta","payload":{"session_id":"session-from-meta","cwd":"/tmp/project"}}"#,
        )
        .unwrap();

        assert_eq!(
            codex_session_id_from_rollout(&path).as_deref(),
            Some("session-from-meta")
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn kimi_session_from_index_uses_current_agents_main_wire() {
        let share = std::env::temp_dir().join(format!("codux-kimi-index-{}", uuid::Uuid::new_v4()));
        let session_dir = share
            .join("sessions")
            .join("wd_lixinhua_hash")
            .join("session_64cd8170-863e-4e63-a89a-15a69e7fb412");
        let agent_dir = session_dir.join("agents").join("main");
        fs::create_dir_all(&agent_dir).unwrap();
        fs::write(agent_dir.join("wire.jsonl"), "{}\n").unwrap();
        fs::write(
            session_dir.join("state.json"),
            r#"{"createdAt":"2026-06-28T07:21:07.730Z"}"#,
        )
        .unwrap();
        fs::write(
            share.join("session_index.jsonl"),
            serde_json::json!({
                "sessionId": "session_64cd8170-863e-4e63-a89a-15a69e7fb412",
                "sessionDir": session_dir.display().to_string(),
                "workDir": "/tmp/project"
            })
            .to_string(),
        )
        .unwrap();

        let resolved = kimi_session_from_index(&share, None, Some("/tmp/project"), None)
            .expect("session from index");

        assert_eq!(resolved, session_dir);
        assert_eq!(
            kimi_wire_path(&resolved),
            resolved.join("agents").join("main").join("wire.jsonl")
        );
        assert_eq!(kimi_state_path(&resolved), resolved.join("state.json"));
        fs::remove_dir_all(share).unwrap();
    }

    #[test]
    fn agy_runtime_db_requires_fresh_file_without_resume_id() {
        let dir = std::env::temp_dir().join(format!("codux-agy-db-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        write_agy_project_db(&dir.join("old-session.db"), "/tmp/project", 100.0);

        let resolved =
            agy_conversation_db_for_runtime_in(&dir, "/tmp/project", None, Some(1_000.0));

        assert!(resolved.is_none());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn agy_runtime_db_allows_exact_resume_id_before_start() {
        let dir = std::env::temp_dir().join(format!("codux-agy-db-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let session_id = "870c1008-fc7b-4366-ad80-185974da837b";
        let path = dir.join(format!("{session_id}.db"));
        write_agy_project_db(&path, "/tmp/project", 100.0);

        let resolved = agy_conversation_db_for_runtime_in(
            &dir,
            "/tmp/project",
            Some(session_id),
            Some(1_000.0),
        );

        assert_eq!(resolved.as_deref(), Some(path.as_path()));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn agy_runtime_db_accepts_new_file_without_resume_id() {
        let dir = std::env::temp_dir().join(format!("codux-agy-db-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("new-session.db");
        write_agy_project_db(&path, "/tmp/project", 1_001.0);

        let resolved =
            agy_conversation_db_for_runtime_in(&dir, "/tmp/project", None, Some(1_000.0));

        assert_eq!(resolved.as_deref(), Some(path.as_path()));
        fs::remove_dir_all(dir).unwrap();
    }

    fn write_agy_project_db(path: &Path, project_path: &str, timestamp: f64) {
        let conn = rusqlite::Connection::open(path).unwrap();
        conn.execute(
            r#"CREATE TABLE trajectory_metadata_blob (
                id text DEFAULT "main",
                data blob,
                PRIMARY KEY (id)
            )"#,
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO trajectory_metadata_blob (id, data) VALUES ('main', ?1)",
            [proto_string_field(7, project_path)],
        )
        .unwrap();
        conn.execute(
            r#"CREATE TABLE steps (
                idx integer,
                step_type integer NOT NULL DEFAULT 0,
                status integer NOT NULL DEFAULT 0,
                metadata blob,
                step_payload blob,
                PRIMARY KEY (idx)
            )"#,
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO steps (idx, step_type, status, metadata, step_payload) VALUES (0, 14, 3, NULL, ?1)",
            [agy_user_step_payload(timestamp)],
        )
        .unwrap();
    }

    fn proto_string_field(number: u64, value: &str) -> Vec<u8> {
        let mut out = proto_varint((number << 3) | 2);
        out.extend(proto_varint(value.len() as u64));
        out.extend(value.as_bytes());
        out
    }

    fn proto_int_field(number: u64, value: u64) -> Vec<u8> {
        let mut out = proto_varint(number << 3);
        out.extend(proto_varint(value));
        out
    }

    fn proto_message_field(number: u64, value: Vec<u8>) -> Vec<u8> {
        let mut out = proto_varint((number << 3) | 2);
        out.extend(proto_varint(value.len() as u64));
        out.extend(value);
        out
    }

    fn agy_user_step_payload(timestamp: f64) -> Vec<u8> {
        let seconds = timestamp.trunc().max(0.0) as u64;
        let nanos = ((timestamp - timestamp.trunc()).max(0.0) * 1_000_000_000.0) as u64;
        let timestamp = [proto_int_field(1, seconds), proto_int_field(2, nanos)].concat();
        let metadata = proto_message_field(1, timestamp);
        let text = proto_string_field(2, "hello");
        [
            proto_int_field(1, 14),
            proto_int_field(4, 3),
            proto_message_field(5, metadata),
            proto_message_field(19, text),
        ]
        .concat()
    }

    fn proto_varint(mut value: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let mut byte = (value & 0x7f) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80;
            }
            out.push(byte);
            if value == 0 {
                return out;
            }
        }
    }
}
