use crate::paths::app_support_dir;
use std::fs;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

static RUNTIME_TRACE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub fn runtime_trace(category: &str, message: &str) {
    let Ok(_guard) = RUNTIME_TRACE_LOCK.get_or_init(|| Mutex::new(())).lock() else {
        return;
    };
    let path = app_support_dir().join("runtime.log");
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
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
