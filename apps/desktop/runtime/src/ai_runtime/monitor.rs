use crate::ai_runtime::{
    constants::TRANSCRIPT_POLL_MINIMUM_SECONDS,
    snapshot::AISessionSnapshot,
    state::{canonical_tool_name, normalized_string},
};
use std::{
    collections::HashMap,
    fs,
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptSignature {
    size: u64,
    modified_millis: u128,
}

#[derive(Debug, Clone)]
pub struct TranscriptMonitor {
    path: String,
    signature: Option<TranscriptSignature>,
    last_poll_at: Option<f64>,
}

pub type TranscriptMonitorMap = Arc<Mutex<HashMap<String, TranscriptMonitor>>>;

pub fn refresh_transcript_monitors(
    monitors: &TranscriptMonitorMap,
    sessions: &[AISessionSnapshot],
) {
    let Ok(mut monitors) = monitors.lock() else {
        return;
    };
    let desired = sessions
        .iter()
        .filter_map(|session| {
            if canonical_tool_name(&session.tool).as_deref() != Some("codex") {
                return None;
            }
            let path = normalized_string(session.transcript_path.as_deref())?;
            Some((session.terminal_id.clone(), path))
        })
        .collect::<HashMap<_, _>>();
    monitors.retain(|terminal_id, _| desired.contains_key(terminal_id));
    for (terminal_id, path) in desired {
        if monitors
            .get(&terminal_id)
            .map(|monitor| monitor.path == path)
            .unwrap_or(false)
        {
            continue;
        }
        monitors.insert(
            terminal_id,
            TranscriptMonitor {
                signature: transcript_signature(Path::new(&path)),
                path,
                last_poll_at: None,
            },
        );
    }
}

pub fn scan_transcript_monitors(
    monitors: &mut HashMap<String, TranscriptMonitor>,
    now: f64,
) -> Vec<String> {
    let mut changed = Vec::new();
    for (terminal_id, monitor) in monitors.iter_mut() {
        let signature = transcript_signature(Path::new(&monitor.path));
        if signature == monitor.signature {
            continue;
        }
        if monitor
            .last_poll_at
            .map(|last_poll_at| now - last_poll_at < TRANSCRIPT_POLL_MINIMUM_SECONDS)
            .unwrap_or(false)
        {
            continue;
        }
        monitor.signature = signature;
        monitor.last_poll_at = Some(now);
        changed.push(terminal_id.clone());
    }
    changed
}

pub fn transcript_signature(path: &Path) -> Option<TranscriptSignature> {
    let metadata = fs::metadata(path).ok()?;
    let modified_millis = metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis();
    Some(TranscriptSignature {
        size: metadata.len(),
        modified_millis,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn scan_transcript_monitor_detects_file_changes_with_cooldown() {
        let dir = std::env::temp_dir().join(format!("codux-transcript-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.jsonl");
        fs::write(&path, "one\n").unwrap();
        let mut monitors = HashMap::from([(
            "term-1".to_string(),
            TranscriptMonitor {
                path: path.display().to_string(),
                signature: transcript_signature(&path),
                last_poll_at: None,
            },
        )]);

        fs::write(&path, "one\ntwo\n").unwrap();
        assert_eq!(
            scan_transcript_monitors(&mut monitors, 100.0),
            vec!["term-1".to_string()]
        );
        fs::write(&path, "one\ntwo\nthree\n").unwrap();
        assert!(scan_transcript_monitors(&mut monitors, 101.0).is_empty());
        assert_eq!(
            scan_transcript_monitors(&mut monitors, 103.0),
            vec!["term-1".to_string()]
        );
        fs::remove_dir_all(dir).unwrap();
    }
}
