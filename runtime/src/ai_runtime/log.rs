use super::paths::runtime_live_log_path;
use crate::runtime_paths::LIVE_LOG_FILE_NAME;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

const RUNTIME_LOG_MAX_BYTES: u64 = 1_000_000;
const RUNTIME_LOG_ROTATION_COUNT: usize = 5;

pub fn reset_runtime_live_log() {
    let path = runtime_live_log_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    clear_runtime_log_family(&path);
    let _ = fs::write(path, "");
}

pub fn runtime_log_line(category: &str, message: &str) {
    let path = runtime_live_log_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    rotate_runtime_log(&path);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| format!("{:.3}", duration.as_secs_f64()))
        .unwrap_or_else(|_| "0.000".to_string());
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "[{timestamp}] [{category}] {message}");
    }
}

fn rotate_runtime_log(path: &Path) {
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if metadata.len() <= RUNTIME_LOG_MAX_BYTES {
        return;
    }
    for index in (1..=RUNTIME_LOG_ROTATION_COUNT).rev() {
        let current = rotated_log_path(path, index);
        if !current.exists() {
            continue;
        }
        if index == RUNTIME_LOG_ROTATION_COUNT {
            let _ = fs::remove_file(&current);
            continue;
        }
        let next = rotated_log_path(path, index + 1);
        let _ = fs::rename(current, next);
    }
    let _ = fs::rename(path, rotated_log_path(path, 1));
}

fn clear_runtime_log_family(path: &Path) {
    let _ = fs::remove_file(path);
    for index in 1..=RUNTIME_LOG_ROTATION_COUNT {
        let _ = fs::remove_file(rotated_log_path(path, index));
    }
}

fn rotated_log_path(path: &Path, index: usize) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| LIVE_LOG_FILE_NAME.to_string());
    path.with_file_name(format!("{file_name}.{index}"))
}
