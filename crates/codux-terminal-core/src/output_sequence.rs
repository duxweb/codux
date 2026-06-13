use std::collections::{HashMap, HashSet};

use crate::TerminalSequence;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalOutputSequenceAction {
    Accept,
    Duplicate,
    Baseline,
}

impl TerminalOutputSequenceAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accept => "accept",
            Self::Duplicate => "duplicate",
            Self::Baseline => "baseline",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalOutputSequenceResult {
    pub action: TerminalOutputSequenceAction,
    pub previous_seq: TerminalSequence,
    /// True when a live frame arrived with `output_seq > previous_seq + 1`,
    /// meaning at least one frame was lost in transit and the session needs a
    /// baseline resync to repair the missing output.
    pub gap: bool,
}

impl TerminalOutputSequenceResult {
    pub fn should_render(&self) -> bool {
        matches!(
            self.action,
            TerminalOutputSequenceAction::Accept | TerminalOutputSequenceAction::Baseline
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct TerminalOutputSequencer {
    seq_by_session: HashMap<String, TerminalSequence>,
    allow_next_live_rebase_sessions: HashSet<String>,
}

impl TerminalOutputSequencer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sequence_for(&self, session_id: &str) -> TerminalSequence {
        self.seq_by_session.get(session_id).copied().unwrap_or(0)
    }

    pub fn observe(
        &mut self,
        session_id: impl AsRef<str>,
        is_buffer: bool,
        output_seq: Option<TerminalSequence>,
        offset: Option<usize>,
        resets_sequence: bool,
    ) -> TerminalOutputSequenceResult {
        let session_id = session_id.as_ref();
        let previous_seq = self.sequence_for(session_id);
        if is_buffer {
            let should_reset = offset.unwrap_or(0) <= 0 || resets_sequence;
            if should_reset {
                self.allow_next_live_rebase_sessions
                    .insert(session_id.to_string());
                if let Some(output_seq) = output_seq {
                    self.seq_by_session
                        .insert(session_id.to_string(), output_seq);
                }
            } else if let Some(output_seq) = output_seq {
                if output_seq >= previous_seq {
                    self.seq_by_session
                        .insert(session_id.to_string(), output_seq);
                }
            }
            return TerminalOutputSequenceResult {
                action: TerminalOutputSequenceAction::Baseline,
                previous_seq,
                gap: false,
            };
        }

        let Some(output_seq) = output_seq else {
            return TerminalOutputSequenceResult {
                action: TerminalOutputSequenceAction::Accept,
                previous_seq,
                gap: false,
            };
        };
        if output_seq <= previous_seq {
            return TerminalOutputSequenceResult {
                action: TerminalOutputSequenceAction::Duplicate,
                previous_seq,
                gap: false,
            };
        }
        let allow_rebase = self.allow_next_live_rebase_sessions.remove(session_id);
        let gap = !allow_rebase && previous_seq > 0 && output_seq > previous_seq + 1;
        if (allow_rebase || previous_seq > 0) && output_seq > previous_seq {
            self.seq_by_session
                .insert(session_id.to_string(), output_seq);
            return TerminalOutputSequenceResult {
                action: TerminalOutputSequenceAction::Accept,
                previous_seq,
                gap,
            };
        }
        self.seq_by_session
            .insert(session_id.to_string(), output_seq);
        self.allow_next_live_rebase_sessions.remove(session_id);
        TerminalOutputSequenceResult {
            action: TerminalOutputSequenceAction::Accept,
            previous_seq,
            gap: false,
        }
    }

    pub fn remove(&mut self, session_id: &str) {
        self.seq_by_session.remove(session_id);
        self.allow_next_live_rebase_sessions.remove(session_id);
    }

    pub fn reset(&mut self) {
        self.seq_by_session.clear();
        self.allow_next_live_rebase_sessions.clear();
    }
}
