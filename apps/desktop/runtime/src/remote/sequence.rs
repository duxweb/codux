use std::collections::{HashMap, VecDeque};

const REMOTE_SEQUENCE_WINDOW: usize = 128;

#[derive(Default)]
pub(crate) struct RemoteSequenceGuard {
    seen_by_channel: HashMap<String, VecDeque<i64>>,
    max_by_channel: HashMap<String, i64>,
}

impl RemoteSequenceGuard {
    pub(crate) fn accept(
        &mut self,
        kind: &str,
        session_id: Option<&str>,
        seq: Option<i64>,
    ) -> bool {
        let Some(seq) = seq else {
            return true;
        };
        let channel = sequence_channel(kind, session_id);
        let max_seq = self.max_by_channel.get(&channel).copied().unwrap_or(0);
        if max_seq > REMOTE_SEQUENCE_WINDOW as i64 && seq <= max_seq - REMOTE_SEQUENCE_WINDOW as i64
        {
            return false;
        }
        let seen = self.seen_by_channel.entry(channel.clone()).or_default();
        if seen.contains(&seq) {
            return false;
        }
        seen.push_back(seq);
        if seq > max_seq {
            self.max_by_channel.insert(channel, seq);
        }
        while seen.len() > REMOTE_SEQUENCE_WINDOW {
            seen.pop_front();
        }
        true
    }
}

fn sequence_channel(kind: &str, session_id: Option<&str>) -> String {
    let session_id = session_id.unwrap_or_default().trim();
    if !session_id.is_empty() {
        return format!("session:{session_id}");
    }
    format!("type:{kind}")
}

#[cfg(test)]
mod tests {
    use super::{REMOTE_SEQUENCE_WINDOW, RemoteSequenceGuard};

    #[test]
    fn accepts_out_of_order_messages_from_different_channels() {
        let mut guard = RemoteSequenceGuard::default();

        assert!(guard.accept("worktree.list", None, Some(34)));
        assert!(guard.accept("project.select", None, Some(33)));
    }

    #[test]
    fn drops_duplicate_in_same_channel() {
        let mut guard = RemoteSequenceGuard::default();

        assert!(guard.accept("project.select", None, Some(33)));
        assert!(!guard.accept("project.select", None, Some(33)));
    }

    #[test]
    fn keeps_terminal_sessions_independent() {
        let mut guard = RemoteSequenceGuard::default();

        assert!(guard.accept("terminal.input", Some("a"), Some(10)));
        assert!(guard.accept("terminal.input", Some("b"), Some(10)));
        assert!(!guard.accept("terminal.input", Some("a"), Some(10)));
    }

    #[test]
    fn rejects_sequences_older_than_sliding_window() {
        let mut guard = RemoteSequenceGuard::default();

        assert!(guard.accept(
            "project.select",
            None,
            Some(REMOTE_SEQUENCE_WINDOW as i64 + 1)
        ));
        assert!(!guard.accept("project.select", None, Some(1)));
    }
}
