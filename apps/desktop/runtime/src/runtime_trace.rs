use crate::runtime_paths::RUNTIME_LOG_FILE_NAME;
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

pub(crate) fn rotated_log_paths(path: &Path) -> Vec<PathBuf> {
    (1..=RUNTIME_LOG_ROTATION_COUNT)
        .map(|index| rotated_log_path(path, index))
        .collect()
}

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
        .unwrap_or_else(|| RUNTIME_LOG_FILE_NAME.to_string());
    path.with_file_name(format!("{file_name}.{index}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotate_runtime_log_preserves_previous_current_log() {
        let directory =
            std::env::temp_dir().join(format!("codux-runtime-log-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("runtime-rust.log");
        fs::write(&path, "current\n").unwrap();
        fs::write(rotated_log_path(&path, 1), "previous\n").unwrap();

        rotate_runtime_log(&path);

        assert!(!path.exists());
        assert_eq!(
            fs::read_to_string(rotated_log_path(&path, 1)).unwrap(),
            "current\n"
        );
        assert_eq!(
            fs::read_to_string(rotated_log_path(&path, 2)).unwrap(),
            "previous\n"
        );

        let _ = fs::remove_dir_all(directory);
    }
}
