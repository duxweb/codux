use crate::runtime_paths::{
    LIVE_LOG_FILE_NAME, RUNTIME_LOG_FILE_NAME, RUNTIME_LOG_PREVIEW_FILE_NAME,
    app_support_candidates, runtime_temp_dir,
};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::Instant,
};

static RUNTIME_TRACE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
static RUNTIME_LOG_STARTED: OnceLock<()> = OnceLock::new();
const RUNTIME_LOG_MAX_BYTES: u64 = 1_000_000;
const RUNTIME_LOG_ROTATION_COUNT: usize = 5;

pub fn runtime_trace(category: &str, message: &str) {
    let Ok(_guard) = RUNTIME_TRACE_LOCK.get_or_init(|| Mutex::new(())).lock() else {
        return;
    };
    let path = crate::runtime_paths::runtime_log_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    prepare_runtime_log(&path);
    let timestamp = chrono::Local::now().to_rfc3339();
    if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "[{timestamp}] [{category}] {message}");
    }
}

pub fn runtime_trace_elapsed(category: &str, action: &str, started_at: Instant, details: &str) {
    let elapsed_ms = started_at.elapsed().as_millis();
    if details.trim().is_empty() {
        runtime_trace(category, &format!("{action} elapsed_ms={elapsed_ms}"));
    } else {
        runtime_trace(
            category,
            &format!("{action} elapsed_ms={elapsed_ms} {details}"),
        );
    }
}

fn prepare_runtime_log(path: &Path) {
    if RUNTIME_LOG_STARTED.get().is_none() {
        let _ = RUNTIME_LOG_STARTED.set(());
        clear_runtime_logs(path);
        return;
    }
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if metadata.len() > RUNTIME_LOG_MAX_BYTES {
        rotate_runtime_log(path);
    }
}

fn clear_runtime_logs(path: &Path) {
    clear_log_family(path);
    for index in 1..=RUNTIME_LOG_ROTATION_COUNT {
        let _ = fs::remove_file(rotated_log_path(path, index));
    }
    for support_dir in app_support_candidates() {
        clear_log_family(&support_dir.join(RUNTIME_LOG_FILE_NAME));
        clear_logs_dir(&support_dir.join("logs"));
    }
    let temp_dir = runtime_temp_dir();
    clear_log_family(&temp_dir.join(LIVE_LOG_FILE_NAME));
    let _ = fs::remove_file(temp_dir.join(RUNTIME_LOG_PREVIEW_FILE_NAME));
}

fn clear_log_family(path: &Path) {
    let _ = fs::remove_file(path);
    let Some(parent) = path.parent() else {
        return;
    };
    let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
        return;
    };
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    let prefix = format!("{file_name}.");
    for entry in entries.flatten() {
        let entry_path = entry.path();
        let Some(name) = entry_path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name.starts_with(&prefix) {
            let _ = fs::remove_file(entry_path);
        }
    }
}

fn clear_logs_dir(path: &Path) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let entry_path = entry.path();
        let Some(name) = entry_path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if name.contains(".log") || name == RUNTIME_LOG_PREVIEW_FILE_NAME {
            let _ = fs::remove_file(entry_path);
        }
    }
}

fn rotate_runtime_log(path: &Path) {
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
    if path.exists() {
        let _ = fs::rename(path, rotated_log_path(path, 1));
    }
}

fn rotated_log_path(path: &Path, index: usize) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| RUNTIME_LOG_FILE_NAME.to_string());
    path.with_file_name(format!("{file_name}.{index}"))
}
