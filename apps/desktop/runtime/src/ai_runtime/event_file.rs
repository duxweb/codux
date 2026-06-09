use crate::ai_runtime::{constants::RUNTIME_EVENT_FILE_MAX_AGE_SECONDS, log::runtime_log_line};
use std::{fs, path::Path};

pub fn clear_runtime_event_dir(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let mut removed = 0;
    for path in entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
    {
        if fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

pub fn drain_runtime_event_dir(dir: &Path, now: f64) -> Vec<Vec<u8>> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut frames = Vec::new();
    let mut paths = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("json"))
        .collect::<Vec<_>>();
    paths.sort_by(|left, right| file_name(left).cmp(&file_name(right)));

    for path in paths {
        let age = fs::metadata(&path)
            .ok()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| now - duration.as_secs_f64())
            .unwrap_or(0.0);
        let data = fs::read(&path).ok();
        let _ = fs::remove_file(&path);
        if age > RUNTIME_EVENT_FILE_MAX_AGE_SECONDS {
            runtime_log_line(
                "hook-file",
                &format!(
                    "drop event-file reason=stale age={age:.1}s file={}",
                    path.display()
                ),
            );
            continue;
        }
        if let Some(data) = data.filter(|value| !value.is_empty()) {
            frames.push(data);
        }
    }
    frames
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn drains_runtime_event_files_and_removes_them() {
        let dir = std::env::temp_dir().join(format!("codux-event-drain-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("one.json"), br#"{"kind":"ai-hook"}"#).unwrap();
        fs::write(dir.join("skip.tmp"), b"ignored").unwrap();

        let frames = drain_runtime_event_dir(&dir, now_seconds());

        assert_eq!(frames, vec![br#"{"kind":"ai-hook"}"#.to_vec()]);
        assert!(!dir.join("one.json").exists());
        assert!(dir.join("skip.tmp").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn clears_runtime_event_files_without_touching_other_files() {
        let dir = std::env::temp_dir().join(format!("codux-event-clear-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("one.json"), br#"{"kind":"ai-hook"}"#).unwrap();
        fs::write(dir.join("two.json"), br#"{"kind":"ai-hook"}"#).unwrap();
        fs::write(dir.join("keep.tmp"), b"ignored").unwrap();

        let removed = clear_runtime_event_dir(&dir);

        assert_eq!(removed, 2);
        assert!(!dir.join("one.json").exists());
        assert!(!dir.join("two.json").exists());
        assert!(dir.join("keep.tmp").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn drains_runtime_event_files_in_file_name_order() {
        let dir = std::env::temp_dir().join(format!("codux-event-order-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("200-turn.json"), br#"{"kind":"turnCompleted"}"#).unwrap();
        fs::write(
            dir.join("100-prompt.json"),
            br#"{"kind":"promptSubmitted"}"#,
        )
        .unwrap();

        let frames = drain_runtime_event_dir(&dir, now_seconds());

        assert_eq!(
            frames,
            vec![
                br#"{"kind":"promptSubmitted"}"#.to_vec(),
                br#"{"kind":"turnCompleted"}"#.to_vec()
            ]
        );
        fs::remove_dir_all(dir).unwrap();
    }

    fn now_seconds() -> f64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs_f64())
            .unwrap_or(0.0)
    }
}
