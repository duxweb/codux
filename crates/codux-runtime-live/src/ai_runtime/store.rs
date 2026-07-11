use super::{
    binding::AIRuntimeBinding,
    constants::{IDLE_SESSION_RETENTION_SECONDS, RUNNING_STALE_SECONDS},
    payload::AIHookEventPayload,
    registry::AIRuntimeTerminalState,
    screen_signal::ScreenSignal,
    snapshot::{
        AIProjectPhase, AIRuntimeCompletionEvent, AIRuntimeContextSnapshot, AIRuntimeStateSnapshot,
        AISessionSnapshot,
    },
    state::canonical_tool_name,
    tool_driver::{process_liveness_tool, screen_starts_idle_tool},
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

use apply::{apply_hook_unlocked, apply_runtime_snapshot_unlocked, apply_screen_signal_unlocked};
use helpers::{
    binding_terminal_session, detected_terminal_session, is_transcript_monitored_session,
    mark_timed_out, now_seconds,
};
pub use helpers::{probe_request_for_session, should_poll_runtime_session};
#[cfg(test)]
use resolve::merge_snapshot_into_hook;
use summary::{
    completed_phase_unlocked, drain_completion_events_unlocked, state_snapshot_unlocked,
};

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
    pub completions: Vec<AIRuntimeCompletionEvent>,
}

impl AIRuntimeStateMutation {
    pub fn merge(&mut self, next: AIRuntimeStateMutation) {
        self.did_change = self.did_change || next.did_change;
        if next.completions.is_empty() {
            if let Some(completion) = next.completion {
                self.push_completion(completion);
            }
        } else {
            for completion in next.completions {
                self.push_completion(completion);
            }
        }
    }

    fn push_completion(&mut self, completion: AIRuntimeCompletionEvent) {
        if self.completion.is_none() {
            self.completion = Some(completion.clone());
        }
        self.completions.push(completion);
    }
}

fn mutation_from_change(did_change: bool, core: &mut AIRuntimeStateCore) -> AIRuntimeStateMutation {
    let mut mutation = AIRuntimeStateMutation {
        did_change,
        completion: None,
        completions: Vec::new(),
    };
    if !did_change {
        return mutation;
    }
    for completion in drain_completion_events_unlocked(core) {
        mutation.push_completion(completion);
    }
    mutation
}

impl AIRuntimeStateStore {
    pub fn snapshot(&self) -> AIRuntimeStateSnapshot {
        let Ok(core) = self.core.lock() else {
            return AIRuntimeStateSnapshot::default();
        };
        state_snapshot_unlocked(&core)
    }

    /// Drop a session when its terminal is explicitly closed so it no longer
    /// lingers in the current-session snapshot. Returns whether an entry was
    /// removed.
    pub fn remove_session(&self, terminal_id: &str) -> bool {
        let Ok(mut core) = self.core.lock() else {
            return false;
        };
        core.sessions.remove(terminal_id).is_some()
    }

    /// Record real terminal output as a liveness heartbeat for an in-flight
    /// turn so the staleness sweep reflects genuine activity instead of a coarse
    /// timer. Deliberately only sustains an existing `responding` turn — it
    /// never starts one — so generic shell or service-command output can never
    /// flip a terminal into AI "responding" status. Returns whether a turn was
    /// refreshed.
    pub fn note_output_activity(&self, terminal_id: &str, now: f64) -> bool {
        let Ok(mut core) = self.core.lock() else {
            return false;
        };
        let Some(session) = core.sessions.get_mut(terminal_id) else {
            return false;
        };
        if session.state != "responding" {
            return false;
        }
        session.updated_at = now;
        true
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
                if session.tool == "kiro" {
                    return now - session.updated_at <= RUNNING_STALE_SECONDS * 3.0;
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
            .filter(|session| is_transcript_monitored_session(session))
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
        self.apply_hook_with_claude_cache(event, None)
    }

    pub(crate) fn apply_hook_with_claude_cache(
        &self,
        event: AIHookEventPayload,
        claude_cache: Option<&mut super::probe::claude::ClaudeProbeCache>,
    ) -> AIRuntimeStateMutation {
        let raw_kind = event.kind.clone();
        let raw_terminal_id = event.terminal_id.clone();
        let previous = self
            .core
            .lock()
            .ok()
            .and_then(|core| core.sessions.get(event.terminal_id.trim()).cloned());
        let event = match claude_cache {
            Some(cache) => {
                resolve::resolve_hook_event_with_claude_cache(event, previous.as_ref(), Some(cache))
            }
            None => resolve::resolve_hook_event(event, previous.as_ref()),
        };
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
        mutation_from_change(did_change || should_notify_running, &mut core)
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
        if did_change && let Some(session) = core.sessions.get(terminal_id) {
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
        mutation_from_change(did_change, &mut core)
    }

    /// Apply the universal screen-scrape signal (see `apply_screen_signal_unlocked`).
    /// Cheap no-op on `Unknown` (the common case) so it can run every poll.
    pub fn apply_screen_signal(
        &self,
        terminal_id: &str,
        signal: ScreenSignal,
    ) -> AIRuntimeStateMutation {
        if matches!(signal, ScreenSignal::Unknown) {
            return AIRuntimeStateMutation::default();
        }
        let Ok(mut core) = self.core.lock() else {
            return AIRuntimeStateMutation::default();
        };
        let allow_idle_start = core
            .sessions
            .get(terminal_id)
            .map(|session| screen_starts_idle_tool(&session.tool))
            .unwrap_or(false);
        let did_change =
            apply_screen_signal_unlocked(&mut core, terminal_id, signal, allow_idle_start);
        mutation_from_change(did_change, &mut core)
    }

    pub fn apply_binding(&self, binding: AIRuntimeBinding) -> AIRuntimeStateMutation {
        let Ok(mut core) = self.core.lock() else {
            return AIRuntimeStateMutation::default();
        };
        let mut next = binding_terminal_session(&binding);
        if let Some(previous) = core.sessions.get(&binding.terminal_id).cloned() {
            let is_new_binding = previous
                .terminal_instance_id
                .as_deref()
                .is_some_and(|value| binding.terminal_instance_id.as_deref() != Some(value))
                || previous
                    .started_at
                    .is_some_and(|started_at| binding.launch_started_at > started_at + 0.001);
            if is_new_binding {
                core.sessions.remove(&binding.terminal_id);
            } else {
                next.state = previous.state;
                next.status = previous.status;
                next.is_running = previous.is_running;
                next.input_tokens = previous.input_tokens;
                next.output_tokens = previous.output_tokens;
                next.cached_input_tokens = previous.cached_input_tokens;
                next.total_tokens = previous.total_tokens;
                next.baseline_total_tokens = previous.baseline_total_tokens;
                next.baseline_cached_input_tokens = previous.baseline_cached_input_tokens;
                next.usage_amounts = previous.usage_amounts;
                next.baseline_usage_amounts = previous.baseline_usage_amounts;
                next.baseline_resolved = previous.baseline_resolved;
                next.active_turn_started_at = previous.active_turn_started_at;
                next.runtime_turn_started_at = previous.runtime_turn_started_at;
                next.completed_turn_started_at = previous.completed_turn_started_at;
                next.has_completed_turn = previous.has_completed_turn;
                next.was_interrupted = previous.was_interrupted;
                next.notification_type = previous.notification_type;
                next.target_tool_name = previous.target_tool_name;
                next.message = previous.message;
                next.latest_assistant_preview = previous.latest_assistant_preview;
                next.plan = previous.plan;
                next.ai_session_id = binding
                    .external_session_id
                    .clone()
                    .or(previous.ai_session_id);
                next.transcript_path = binding.transcript_path.clone().or(previous.transcript_path);
                next.model = binding.model.clone().or(previous.model);
                next.updated_at = previous.updated_at.max(next.updated_at);
            }
        }

        if core.sessions.get(&binding.terminal_id) == Some(&next) {
            return AIRuntimeStateMutation::default();
        }
        super::runtime_log_line(
            "runtime-binding",
            &format!(
                "bind terminal={} tool={} external={} transcript={}",
                next.terminal_id,
                next.tool,
                next.ai_session_id.as_deref().unwrap_or("none"),
                next.transcript_path.as_deref().unwrap_or("none")
            ),
        );
        core.sessions.insert(binding.terminal_id, next);
        AIRuntimeStateMutation {
            did_change: true,
            ..Default::default()
        }
    }

    /// Create idle sessions for terminals whose AI tool was process-detected but have none yet; the probe + screen refine them. `detected`: terminal_id -> tool.
    pub fn ensure_detected_sessions(
        &self,
        terminals: &[AIRuntimeTerminalState],
        detected: &HashMap<String, String>,
        now: f64,
    ) -> AIRuntimeStateMutation {
        if detected.is_empty() {
            return AIRuntimeStateMutation::default();
        }
        let Ok(mut core) = self.core.lock() else {
            return AIRuntimeStateMutation::default();
        };
        let mut did_change = false;
        for terminal in terminals {
            let Some(tool) = detected.get(&terminal.terminal_id) else {
                continue;
            };
            if let Some(existing) = core.sessions.get_mut(&terminal.terminal_id) {
                if canonical_tool_name(&existing.tool) == canonical_tool_name(tool) {
                    if existing.state == "idle"
                        && !existing.has_completed_turn
                        && !existing.was_interrupted
                        && terminal.terminal_instance_id.is_some()
                        && existing.terminal_instance_id == terminal.terminal_instance_id
                        && existing.updated_at < now
                    {
                        existing.updated_at = now;
                        did_change = true;
                    }
                } else if let Some(session) = detected_terminal_session(terminal, tool, now) {
                    super::runtime_log_line(
                        "runtime-state",
                        &format!(
                            "switch terminal={} from={} to={} state=idle",
                            terminal.terminal_id, existing.tool, session.tool
                        ),
                    );
                    core.sessions.insert(terminal.terminal_id.clone(), session);
                    did_change = true;
                }
                continue;
            }
            if let Some(session) = detected_terminal_session(terminal, tool, now) {
                super::runtime_log_line(
                    "runtime-state",
                    &format!(
                        "detect terminal={} tool={} state=idle",
                        session.terminal_id, session.tool
                    ),
                );
                core.sessions.insert(terminal.terminal_id.clone(), session);
                did_change = true;
            }
        }
        AIRuntimeStateMutation {
            did_change,
            ..Default::default()
        }
    }

    pub fn retire_undetected_hookless_sessions(
        &self,
        terminals: &[AIRuntimeTerminalState],
        shell_pids: &[(String, u32)],
        detected: &HashMap<String, String>,
        now: f64,
    ) -> AIRuntimeStateMutation {
        let shell_terminal_ids = shell_pids
            .iter()
            .map(|(terminal_id, _)| terminal_id.as_str())
            .collect::<HashSet<_>>();
        if shell_terminal_ids.is_empty() {
            return AIRuntimeStateMutation::default();
        }
        let live_terminal_ids = terminals
            .iter()
            .map(|terminal| terminal.terminal_id.as_str())
            .collect::<HashSet<_>>();
        let Ok(mut core) = self.core.lock() else {
            return AIRuntimeStateMutation::default();
        };
        let retiring_ids = core
            .sessions
            .iter()
            .filter_map(|(terminal_id, session)| {
                if !process_liveness_tool(&session.tool)
                    || !live_terminal_ids.contains(terminal_id.as_str())
                    || !shell_terminal_ids.contains(terminal_id.as_str())
                    || detected.get(terminal_id) == Some(&session.tool)
                    || now - session.updated_at <= 6.0
                {
                    return None;
                }
                Some(terminal_id.clone())
            })
            .collect::<Vec<_>>();
        let mut did_change = false;
        for terminal_id in retiring_ids {
            let Some(session) = core.sessions.get(&terminal_id).cloned() else {
                continue;
            };
            if matches!(session.state.as_str(), "responding" | "needsInput") {
                let updated_at = session.updated_at;
                core.sessions
                    .insert(terminal_id.clone(), mark_timed_out(session, updated_at));
            } else {
                core.sessions.remove(&terminal_id);
            }
            did_change = true;
        }
        AIRuntimeStateMutation {
            did_change,
            ..Default::default()
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
            // Creation is owned by `ensure_detected_sessions`; reconcile only ages/cleans existing ones.
            let Some(existing) = core.sessions.get(&terminal.terminal_id).cloned() else {
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
                core.sessions
                    .insert(terminal.terminal_id.clone(), mark_timed_out(existing, now));
                did_change = true;
            }
        }

        let stale_ids = core
            .sessions
            .iter()
            .filter(|(terminal_id, session)| {
                !live_terminal_ids.contains(terminal_id.as_str()) && session.state != "idle"
            })
            .map(|(terminal_id, _)| terminal_id.clone())
            .collect::<Vec<_>>();
        for terminal_id in stale_ids {
            if let Some(session) = core.sessions.get(&terminal_id).cloned() {
                core.sessions
                    .insert(terminal_id, mark_timed_out(session, now));
                did_change = true;
            }
        }

        // Reclaim orphans: idle sessions whose terminal is gone and that have
        // sat untouched past the retention window. Explicit closes are handled
        // immediately by `remove_session`; this only bounds growth from crashes
        // / abnormal terminal disappearance so `sessions` can't leak forever.
        let expired_ids = core
            .sessions
            .iter()
            .filter(|(terminal_id, session)| {
                !live_terminal_ids.contains(terminal_id.as_str())
                    && session.state == "idle"
                    && now - session.updated_at > IDLE_SESSION_RETENTION_SECONDS
            })
            .map(|(terminal_id, _)| terminal_id.clone())
            .collect::<Vec<_>>();
        for terminal_id in expired_ids {
            core.sessions.remove(&terminal_id);
            did_change = true;
        }

        mutation_from_change(did_change, &mut core)
    }
}
