use crate::paths::app_support_dir;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

static RUNTIME_TRACE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
const RUNTIME_LOG_MAX_BYTES: u64 = 1_000_000;
const RUNTIME_LOG_ROTATION_COUNT: usize = 5;
static RUNTIME_LOG_STARTED: OnceLock<()> = OnceLock::new();

pub fn runtime_trace(category: &str, message: &str) {
    let Ok(_guard) = RUNTIME_TRACE_LOCK.get_or_init(|| Mutex::new(())).lock() else {
        return;
    };
    let path = app_support_dir().join("runtime.log");
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
        rotate_runtime_log(path);
        return;
    }
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if metadata.len() > RUNTIME_LOG_MAX_BYTES {
        rotate_runtime_log(path);
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
        .unwrap_or_else(|| "runtime.log".to_string());
    path.with_file_name(format!("{file_name}.{index}"))
}
