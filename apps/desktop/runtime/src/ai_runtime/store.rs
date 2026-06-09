use super::{
    constants::RUNNING_STALE_SECONDS,
    payload::AIHookEventPayload,
    registry::AIRuntimeTerminalState,
    snapshot::{
        AIProjectPhase, AIRuntimeCompletionEvent, AIRuntimeContextSnapshot, AIRuntimeStateSnapshot,
        AISessionSnapshot,
    },
};
use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
};

mod apply;
mod helpers;
mod resolve;
mod summary;

#[cfg(test)]
mod tests;

use apply::{apply_hook_unlocked, apply_runtime_snapshot_unlocked};
use helpers::{
    bridge_terminal_session, is_codex_transcript_session, mark_interrupted, now_seconds,
};
pub use helpers::{probe_request_for_session, should_poll_runtime_session};
#[cfg(test)]
use resolve::merge_snapshot_into_hook;
use resolve::resolve_hook_event;
use summary::{completed_phase_unlocked, next_completion_event_unlocked, state_snapshot_unlocked};

#[derive(Default)]
struct AIRuntimeStateCore {
    sessions: HashMap<String, AISessionSnapshot>,
    logical_baselines: HashMap<String, i64>,
    logical_cached_baselines: HashMap<String, i64>,
    dismissed_completed_at: HashMap<String, f64>,
    latest_active_started_at_by_project: HashMap<String, f64>,
    notified_completion_keys: HashSet<String>,
}

#[derive(Default)]
pub struct AIRuntimeStateStore {
    core: Mutex<AIRuntimeStateCore>,
}

#[derive(Default)]
pub struct AIRuntimeStateMutation {
    pub did_change: bool,
    pub completion: Option<AIRuntimeCompletionEvent>,
}

impl AIRuntimeStateMutation {
    pub fn merge(&mut self, next: AIRuntimeStateMutation) {
        self.did_change = self.did_change || next.did_change;
        match (&self.completion, next.completion) {
            (None, Some(candidate)) => self.completion = Some(candidate),
            (Some(current), Some(candidate)) if candidate.id > current.id => {
                self.completion = Some(candidate);
            }
            _ => {}
        }
    }
}

impl AIRuntimeStateStore {
    pub fn snapshot(&self) -> AIRuntimeStateSnapshot {
        let Ok(core) = self.core.lock() else {
            return AIRuntimeStateSnapshot::default();
        };
        state_snapshot_unlocked(&core)
    }

    pub fn runtime_tracked_sessions(&self, now: f64) -> Vec<AISessionSnapshot> {
        let Ok(core) = self.core.lock() else {
            return Vec::new();
        };
        core.sessions
            .values()
            .filter(|session| {
                if session.state == "responding" || session.state == "needsInput" {
                    return true;
                }
                !session.has_completed_turn
                    && now - session.updated_at <= RUNNING_STALE_SECONDS * 3.0
            })
            .cloned()
            .collect()
    }

    pub fn transcript_monitored_sessions(&self) -> Vec<AISessionSnapshot> {
        let Ok(core) = self.core.lock() else {
            return Vec::new();
        };
        core.sessions
            .values()
            .filter(|session| is_codex_transcript_session(session))
            .cloned()
            .collect()
    }

    pub fn sessions_for_terminals(&self, terminal_ids: &HashSet<String>) -> Vec<AISessionSnapshot> {
        let Ok(core) = self.core.lock() else {
            return Vec::new();
        };
        core.sessions
            .values()
            .filter(|session| terminal_ids.contains(&session.terminal_id))
            .cloned()
            .collect()
    }

    pub fn dismiss_completion(&self, project_id: &str) -> bool {
        let Ok(mut core) = self.core.lock() else {
            return false;
        };
        let AIProjectPhase::Completed { updated_at, .. } =
            completed_phase_unlocked(&core, project_id, now_seconds())
        else {
            return false;
        };
        let previous = core
            .dismissed_completed_at
            .get(project_id)
            .copied()
            .unwrap_or(0.0);
        let next = previous.max(updated_at);
        if next <= previous {
            return false;
        }
        core.dismissed_completed_at
            .insert(project_id.to_string(), next);
        true
    }

    pub fn apply_hook(&self, event: AIHookEventPayload) -> AIRuntimeStateMutation {
        let raw_kind = event.kind.clone();
        let raw_terminal_id = event.terminal_id.clone();
        let previous = self
            .core
            .lock()
            .ok()
            .and_then(|core| core.sessions.get(event.terminal_id.trim()).cloned());
        let event = resolve_hook_event(event, previous.as_ref());
        if raw_kind != event.kind {
            super::runtime_log_line(
                "runtime-ingress",
                &format!(
                    "resolve hook terminal={} raw_kind={} resolved_kind={} previous_state={} ai_session={}",
                    raw_terminal_id,
                    raw_kind,
                    event.kind,
                    previous
                        .as_ref()
                        .map(|session| session.state.as_str())
                        .unwrap_or("none"),
                    event.ai_session_id.as_deref().unwrap_or("none")
                ),
            );
        }
        let Ok(mut core) = self.core.lock() else {
            return AIRuntimeStateMutation::default();
        };
        let did_change = apply_hook_unlocked(&mut core, event);
        let should_notify_running = !did_change
            && raw_kind == "promptSubmitted"
            && core
                .sessions
                .get(raw_terminal_id.trim())
                .map(|session| session.state == "responding")
                .unwrap_or(false);
        if did_change || should_notify_running {
            if let Some(session) = core.sessions.get(raw_terminal_id.trim()) {
                super::runtime_log_line(
                    "runtime-state",
                    &format!(
                        "hook terminal={} state={} completed={} interrupted={} updated_at={:.3} kind={} notify_running={}",
                        session.terminal_id,
                        session.state,
                        session.has_completed_turn,
                        session.was_interrupted,
                        session.updated_at,
                        raw_kind,
                        should_notify_running
                    ),
                );
            }
        } else if raw_kind == "promptSubmitted" {
            let session_state = core
                .sessions
                .get(raw_terminal_id.trim())
                .map(|session| session.state.as_str())
                .unwrap_or("none");
            super::runtime_log_line(
                "runtime-state",
                &format!(
                    "hook no-change terminal={} kind={} current_state={}",
                    raw_terminal_id, raw_kind, session_state
                ),
            );
        }
        AIRuntimeStateMutation {
            did_change: did_change || should_notify_running,
            completion: did_change
                .then(|| next_completion_event_unlocked(&mut core))
                .flatten(),
        }
    }

    pub fn apply_runtime_snapshot(
        &self,
        terminal_id: &str,
        snapshot: AIRuntimeContextSnapshot,
    ) -> AIRuntimeStateMutation {
        let snapshot_response_state = snapshot.response_state.clone();
        let snapshot_completed_at = snapshot.completed_at;
        let snapshot_updated_at = snapshot.updated_at;
        let snapshot_has_completed = snapshot.has_completed_turn;
        let snapshot_was_interrupted = snapshot.was_interrupted;
        let Ok(mut core) = self.core.lock() else {
            return AIRuntimeStateMutation::default();
        };
        let did_change = apply_runtime_snapshot_unlocked(&mut core, terminal_id, snapshot);
        if did_change {
            if let Some(session) = core.sessions.get(terminal_id) {
                super::runtime_log_line(
                    "runtime-state",
                    &format!(
                        "snapshot terminal={} state={} completed={} interrupted={} updated_at={:.3} response_state={} snapshot_completed_at={} snapshot_updated_at={:.3} snapshot_completed={} snapshot_interrupted={}",
                        session.terminal_id,
                        session.state,
                        session.has_completed_turn,
                        session.was_interrupted,
                        session.updated_at,
                        snapshot_response_state.as_deref().unwrap_or("none"),
                        snapshot_completed_at
                            .map(|value| format!("{value:.3}"))
                            .unwrap_or_else(|| "none".to_string()),
                        snapshot_updated_at,
                        snapshot_has_completed,
                        snapshot_was_interrupted
                    ),
                );
            }
        }
        AIRuntimeStateMutation {
            did_change,
            completion: did_change
                .then(|| next_completion_event_unlocked(&mut core))
                .flatten(),
        }
    }

    pub fn reconcile_bridge_snapshot(
        &self,
        terminals: &[AIRuntimeTerminalState],
    ) -> AIRuntimeStateMutation {
        let Ok(mut core) = self.core.lock() else {
            return AIRuntimeStateMutation::default();
        };
        let now = now_seconds();
        let live_terminal_ids = terminals
            .iter()
            .map(|terminal| terminal.terminal_id.as_str())
            .collect::<HashSet<_>>();
        let mut did_change = false;

        for terminal in terminals {
            let Some(existing) = core.sessions.get(&terminal.terminal_id).cloned() else {
                if let Some(session) = bridge_terminal_session(terminal, now) {
                    core.sessions.insert(terminal.terminal_id.clone(), session);
                    did_change = true;
                }
                continue;
            };
            if existing.state != "responding" {
                continue;
            }
            if terminal.terminal_instance_id.is_some()
                && existing.terminal_instance_id != terminal.terminal_instance_id
            {
                core.sessions.remove(&terminal.terminal_id);
                did_change = true;
                continue;
            }
            if now - existing.updated_at > RUNNING_STALE_SECONDS {
                core.sessions.insert(
                    terminal.terminal_id.clone(),
                    mark_interrupted(existing, now),
                );
                did_change = true;
            }
        }

        let stale_ids = core
            .sessions
            .iter()
            .filter_map(|(terminal_id, session)| {
                (!live_terminal_ids.contains(terminal_id.as_str()) && session.state != "idle")
                    .then(|| terminal_id.clone())
            })
            .collect::<Vec<_>>();
        for terminal_id in stale_ids {
            if let Some(session) = core.sessions.get(&terminal_id).cloned() {
                core.sessions
                    .insert(terminal_id, mark_interrupted(session, now));
                did_change = true;
            }
        }

        AIRuntimeStateMutation {
            did_change,
            completion: did_change
                .then(|| next_completion_event_unlocked(&mut core))
                .flatten(),
        }
    }
}
