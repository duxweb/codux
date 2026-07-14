use crate::ai_runtime::{
    binding::{
        AIRuntimeBindingFileEvent, AIRuntimeBindingScanState, read_changed_runtime_binding,
        runtime_file_signature, scan_runtime_bindings,
    },
    constants::{POLL_INTERVAL_SECONDS, TRANSCRIPT_MONITOR_INTERVAL_MS},
    event_file::drain_runtime_event_dir,
    frame::runtime_frame_to_hook,
    log::runtime_log_line,
    monitor::{TranscriptMonitorMap, refresh_transcript_monitors, scan_transcript_monitors},
    payload::AIRuntimeEvent,
    probe::{claude::ClaudeProbeCache, probe_runtime_with_claude_cache},
    registry::AIRuntimeRegistry,
    screen_signal::detect_screen_signal,
    snapshot::{AIRuntimeCompletionEvent, AIRuntimeStateSnapshot},
    state::canonical_tool_name,
    store::{AIRuntimeStateMutation, AIRuntimeStateStore},
    store::{probe_request_for_session, should_poll_runtime_session},
    terminal_status::{TERMINAL_COMMAND_OSC_SOURCE, TerminalStatusEvent},
    tool_driver::{runtime_screen_patterns, screen_starts_idle_tool},
};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, SyncSender, channel, sync_channel},
    },
    thread,
};

#[derive(Debug)]
enum AIRuntimeSupervisorMessage {
    HookFrame(Vec<u8>),
    DrainEventDir,
    Poll,
    ScreenSignal(String),
    TerminalStatus(TerminalStatusEvent),
    ScanBindings,
    ScanBindingFile(AIRuntimeBindingFileEvent),
    TranscriptTail(Vec<String>),
    RemoveSession {
        terminal_id: String,
        reply: std::sync::mpsc::Sender<bool>,
    },
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AIRuntimeSupervisorEvent {
    RuntimeEvent {
        event: Box<AIRuntimeEvent>,
    },
    State {
        snapshot: Box<AIRuntimeStateSnapshot>,
    },
    Completion {
        completion: Box<AIRuntimeCompletionEvent>,
    },
    TerminalStatus {
        status: TerminalStatusEvent,
    },
}

pub struct AIRuntimeSupervisor {
    tx: SyncSender<AIRuntimeSupervisorMessage>,
    rx: Mutex<Option<Receiver<AIRuntimeSupervisorMessage>>>,
    state: Arc<AIRuntimeStateStore>,
    transcript_monitors: TranscriptMonitorMap,
    binding_scan: Arc<Mutex<AIRuntimeBindingScanState>>,
    events: Arc<Mutex<Vec<AIRuntimeSupervisorEvent>>>,
    terminal_statuses: Arc<Mutex<HashMap<String, TerminalStatusEvent>>>,
    started: Mutex<bool>,
}

struct AIRuntimeSupervisorLoopContext {
    registry: Arc<AIRuntimeRegistry>,
    state: Arc<AIRuntimeStateStore>,
    transcript_monitors: TranscriptMonitorMap,
    binding_scan: Arc<Mutex<AIRuntimeBindingScanState>>,
    events: Arc<Mutex<Vec<AIRuntimeSupervisorEvent>>>,
    terminal_statuses: Arc<Mutex<HashMap<String, TerminalStatusEvent>>>,
    runtime_event_dir: PathBuf,
    binding_dir: PathBuf,
}

impl AIRuntimeSupervisor {
    pub fn new() -> Self {
        let (tx, rx) = sync_channel(1024);
        Self {
            tx,
            rx: Mutex::new(Some(rx)),
            state: Arc::new(AIRuntimeStateStore::default()),
            transcript_monitors: Arc::new(Mutex::new(Default::default())),
            binding_scan: Arc::new(Mutex::new(Default::default())),
            events: Arc::new(Mutex::new(Vec::new())),
            terminal_statuses: Arc::new(Mutex::new(HashMap::new())),
            started: Mutex::new(false),
        }
    }

    pub fn start(
        &self,
        registry: Arc<AIRuntimeRegistry>,
        runtime_event_dir: PathBuf,
        binding_dir: PathBuf,
    ) -> Result<(), String> {
        let mut started = self
            .started
            .lock()
            .map_err(|_| "AI runtime supervisor start lock poisoned.".to_string())?;
        if *started {
            return Ok(());
        }
        let mut receiver = self
            .rx
            .lock()
            .map_err(|_| "AI runtime supervisor lock poisoned.".to_string())?;
        let Some(rx) = receiver.take() else {
            return Ok(());
        };
        start_poll_loop(self.tx.clone());
        start_transcript_monitor_loop(self.tx.clone(), Arc::clone(&self.transcript_monitors));
        start_event_dir_watcher(self.tx.clone(), runtime_event_dir.clone());
        start_binding_dir_watcher(self.tx.clone(), binding_dir.clone());
        let _ = self.tx.try_send(AIRuntimeSupervisorMessage::ScanBindings);
        let context = AIRuntimeSupervisorLoopContext {
            registry,
            state: Arc::clone(&self.state),
            transcript_monitors: Arc::clone(&self.transcript_monitors),
            binding_scan: Arc::clone(&self.binding_scan),
            events: Arc::clone(&self.events),
            terminal_statuses: Arc::clone(&self.terminal_statuses),
            runtime_event_dir,
            binding_dir,
        };
        let spawn_result = thread::Builder::new()
            .name("codux-ai-runtime-supervisor".to_string())
            .spawn(move || supervisor_loop(rx, context));
        match spawn_result {
            Ok(_) => {
                *started = true;
                Ok(())
            }
            Err(error) => Err(error.to_string()),
        }
    }

    pub fn submit_frame(&self, frame: Vec<u8>) -> Result<(), String> {
        self.tx
            .send(AIRuntimeSupervisorMessage::HookFrame(frame))
            .map_err(|error| error.to_string())
    }

    pub fn poll_once(&self) -> Result<(), String> {
        self.tx
            .send(AIRuntimeSupervisorMessage::Poll)
            .map_err(|error| error.to_string())
    }

    pub fn state_snapshot(&self) -> AIRuntimeStateSnapshot {
        self.state.snapshot()
    }

    pub fn dismiss_completion(&self, project_id: &str) -> bool {
        self.state.dismiss_completion(project_id)
    }

    pub fn remove_session(&self, terminal_id: &str) -> bool {
        let Ok(started) = self.started.lock() else {
            return false;
        };
        if !*started {
            if let Ok(mut statuses) = self.terminal_statuses.lock() {
                statuses.remove(terminal_id);
            }
            return self.state.remove_session(terminal_id);
        }
        drop(started);
        let (reply, result) = channel();
        if self
            .tx
            .send(AIRuntimeSupervisorMessage::RemoveSession {
                terminal_id: terminal_id.to_string(),
                reply,
            })
            .is_err()
        {
            return false;
        }
        result.recv().unwrap_or(false)
    }

    pub fn note_output_activity(&self, terminal_id: &str, now: f64) -> bool {
        self.state.note_output_activity(terminal_id, now)
    }

    pub fn refresh_screen_signal(&self, terminal_id: &str) -> bool {
        self.tx
            .try_send(AIRuntimeSupervisorMessage::ScreenSignal(
                terminal_id.to_string(),
            ))
            .is_ok()
    }

    pub fn submit_terminal_status(&self, status: TerminalStatusEvent) -> Result<(), String> {
        self.tx
            .send(AIRuntimeSupervisorMessage::TerminalStatus(status))
            .map_err(|error| error.to_string())
    }

    pub fn drain_events(&self) -> Vec<AIRuntimeSupervisorEvent> {
        self.events
            .lock()
            .map(|mut events| events.drain(..).collect())
            .unwrap_or_default()
    }

    pub fn terminal_statuses_snapshot(&self) -> Vec<TerminalStatusEvent> {
        let mut statuses: Vec<TerminalStatusEvent> = self
            .terminal_statuses
            .lock()
            .map(|statuses| statuses.values().cloned().collect())
            .unwrap_or_default();
        statuses.sort_by(|left, right| {
            right
                .updated_at
                .total_cmp(&left.updated_at)
                .then_with(|| left.terminal_id.cmp(&right.terminal_id))
        });
        statuses
    }
}

impl Default for AIRuntimeSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

fn supervisor_loop(
    rx: Receiver<AIRuntimeSupervisorMessage>,
    context: AIRuntimeSupervisorLoopContext,
) {
    let AIRuntimeSupervisorLoopContext {
        registry,
        state,
        transcript_monitors,
        binding_scan,
        events,
        terminal_statuses,
        runtime_event_dir,
        binding_dir,
    } = context;
    let mut claude_cache = ClaudeProbeCache::default();
    while let Ok(message) = rx.recv() {
        match message {
            AIRuntimeSupervisorMessage::HookFrame(frame) => {
                handle_hook_frame(
                    &frame,
                    &state,
                    &transcript_monitors,
                    &events,
                    &mut claude_cache,
                );
            }
            AIRuntimeSupervisorMessage::DrainEventDir => {
                // Single-threaded drain: both the FS watcher and the periodic
                // fallback funnel here, so files are read+deleted exactly once
                // (no cross-thread double-read / duplicate hooks).
                for frame in drain_runtime_event_dir(&runtime_event_dir, now_seconds()) {
                    handle_hook_frame(
                        &frame,
                        &state,
                        &transcript_monitors,
                        &events,
                        &mut claude_cache,
                    );
                }
            }
            AIRuntimeSupervisorMessage::ScanBindings => {
                let mutation = handle_binding_scan(&state, &binding_scan, &binding_dir);
                after_mutation(&state, &transcript_monitors, &events, mutation);
            }
            AIRuntimeSupervisorMessage::ScanBindingFile(file_event) => {
                let mutation = handle_binding_file_event(&state, &binding_scan, file_event);
                after_mutation(&state, &transcript_monitors, &events, mutation);
            }
            AIRuntimeSupervisorMessage::Poll => {
                let mutation =
                    poll_runtime_sessions(&state, &registry, "interval", None, &mut claude_cache);
                after_mutation(&state, &transcript_monitors, &events, mutation);
            }
            AIRuntimeSupervisorMessage::ScreenSignal(terminal_id) => {
                let mutation = apply_screen_signal_for_terminal(&state, &registry, &terminal_id);
                after_mutation(&state, &transcript_monitors, &events, mutation);
            }
            AIRuntimeSupervisorMessage::TerminalStatus(status) => {
                if status.source != TERMINAL_COMMAND_OSC_SOURCE
                    && let Ok(mut statuses) = terminal_statuses.lock()
                {
                    let should_update = statuses
                        .get(&status.terminal_id)
                        .map(|current| status.updated_at >= current.updated_at)
                        .unwrap_or(true);
                    if should_update {
                        statuses.insert(status.terminal_id.clone(), status.clone());
                    }
                }
                push_event(&events, AIRuntimeSupervisorEvent::TerminalStatus { status });
            }
            AIRuntimeSupervisorMessage::TranscriptTail(terminal_ids) => {
                let terminal_ids = terminal_ids.into_iter().collect::<HashSet<_>>();
                if terminal_ids.is_empty() {
                    continue;
                }
                let mutation = poll_runtime_sessions(
                    &state,
                    &registry,
                    "transcript-tail",
                    Some(&terminal_ids),
                    &mut claude_cache,
                );
                after_mutation(&state, &transcript_monitors, &events, mutation);
            }
            AIRuntimeSupervisorMessage::RemoveSession { terminal_id, reply } => {
                let mutation = AIRuntimeStateMutation {
                    did_change: state.remove_session(&terminal_id),
                    ..Default::default()
                };
                let removed = mutation.did_change;
                if let Ok(mut statuses) = terminal_statuses.lock() {
                    statuses.remove(&terminal_id);
                }
                after_mutation(&state, &transcript_monitors, &events, mutation);
                let _ = reply.send(removed);
            }
        }
    }
}

fn handle_binding_scan(
    state: &AIRuntimeStateStore,
    binding_scan: &Arc<Mutex<AIRuntimeBindingScanState>>,
    binding_dir: &Path,
) -> AIRuntimeStateMutation {
    let events = binding_scan
        .lock()
        .map(|mut scan| scan_runtime_bindings(binding_dir, &mut scan))
        .unwrap_or_default();
    let mut mutation = AIRuntimeStateMutation::default();
    for event in events {
        runtime_log_line(
            "runtime-binding",
            &format!(
                "event path={} tool={} terminal={} size={} modified={}",
                event.path.display(),
                event.binding.tool,
                event.binding.terminal_id,
                event.size,
                event.modified_millis
            ),
        );
        mutation.merge(state.apply_binding(event.binding));
    }
    mutation
}

fn handle_binding_file_event(
    state: &AIRuntimeStateStore,
    binding_scan: &Arc<Mutex<AIRuntimeBindingScanState>>,
    file_event: AIRuntimeBindingFileEvent,
) -> AIRuntimeStateMutation {
    let event = binding_scan.lock().ok().and_then(|mut scan| {
        read_changed_runtime_binding(file_event.path, file_event.signature, &mut scan)
    });
    let Some(event) = event else {
        return AIRuntimeStateMutation::default();
    };
    runtime_log_line(
        "runtime-binding",
        &format!(
            "event path={} tool={} terminal={} size={} modified={}",
            event.path.display(),
            event.binding.tool,
            event.binding.terminal_id,
            event.size,
            event.modified_millis
        ),
    );
    state.apply_binding(event.binding)
}

fn handle_hook_frame(
    frame: &[u8],
    state: &AIRuntimeStateStore,
    transcript_monitors: &TranscriptMonitorMap,
    events: &Arc<Mutex<Vec<AIRuntimeSupervisorEvent>>>,
    claude_cache: &mut ClaudeProbeCache,
) {
    let Some(payload) = runtime_frame_to_hook(frame) else {
        runtime_log_line(
            "runtime-ingress",
            &format!("drop hook-frame reason=decode bytes={}", frame.len()),
        );
        return;
    };
    if canonical_tool_name(&payload.tool).as_deref() == Some("agy") {
        runtime_log_line(
            "runtime-ingress",
            &format!(
                "drop hook-frame reason=agy-db-only kind={} terminal={} project={}",
                payload.kind, payload.terminal_id, payload.project_id
            ),
        );
        return;
    }
    runtime_log_line(
        "runtime-ingress",
        &format!(
            "receive hook tool={} kind={} terminal={} project={}",
            payload.tool, payload.kind, payload.terminal_id, payload.project_id
        ),
    );
    push_event(
        events,
        AIRuntimeSupervisorEvent::RuntimeEvent {
            event: Box::new(AIRuntimeEvent::Hook {
                payload: payload.clone(),
            }),
        },
    );
    let mutation = state.apply_hook_with_claude_cache(payload, Some(claude_cache));
    runtime_log_line(
        "runtime-ingress",
        if mutation.did_change {
            "apply hook result=changed"
        } else {
            "apply hook result=no-change"
        },
    );
    after_mutation(state, transcript_monitors, events, mutation);
}

fn after_mutation(
    state: &AIRuntimeStateStore,
    transcript_monitors: &TranscriptMonitorMap,
    events: &Arc<Mutex<Vec<AIRuntimeSupervisorEvent>>>,
    mutation: AIRuntimeStateMutation,
) {
    // The monitored-session set only shifts when the state changed, so skip the
    // lock + full codex-session clone on the (common) no-op message.
    if mutation.did_change {
        refresh_transcript_monitors(transcript_monitors, &state.transcript_monitored_sessions());
        let snapshot = state.snapshot();
        push_event(
            events,
            AIRuntimeSupervisorEvent::State {
                snapshot: Box::new(snapshot),
            },
        );
    }
    let completions = if mutation.completions.is_empty() {
        mutation.completion.into_iter().collect::<Vec<_>>()
    } else {
        mutation.completions
    };
    for completion in completions {
        push_event(
            events,
            AIRuntimeSupervisorEvent::Completion {
                completion: Box::new(completion),
            },
        );
    }
}

fn poll_runtime_sessions(
    state: &AIRuntimeStateStore,
    registry: &AIRuntimeRegistry,
    reason: &str,
    terminal_ids: Option<&HashSet<String>>,
    claude_cache: &mut ClaudeProbeCache,
) -> AIRuntimeStateMutation {
    let terminal_snapshot = registry.snapshot();
    let mut mutation = state.reconcile_bridge_snapshot(&terminal_snapshot);
    let now = now_seconds();
    // Interval poll only (keeps `ps` to once per interval): detect each terminal's AI tool and create an idle session the loop below refines.
    if terminal_ids.is_none() && detection_enabled() {
        let shell_pids = registry.shell_pids_snapshot();
        if let Some(detected) =
            crate::ai_runtime::process_detect::detect_terminal_tools(&shell_pids)
        {
            mutation.merge(state.ensure_detected_sessions(&terminal_snapshot, &detected, now));
            mutation.merge(state.retire_undetected_hookless_sessions(
                &terminal_snapshot,
                &shell_pids,
                &detected,
                now,
            ));
        }
    }
    let sessions = terminal_ids
        .map(|ids| state.sessions_for_terminals(ids))
        .unwrap_or_else(|| state.runtime_tracked_sessions(now));
    if terminal_ids.is_none() {
        let tracked = sessions
            .iter()
            .map(|session| session.terminal_id.clone())
            .collect::<HashSet<_>>();
        claude_cache.retain_terminals(&tracked);
    }
    let mut assigned_external_session_ids = sessions
        .iter()
        .filter_map(|session| {
            session.ai_session_id.as_ref().map(|external_session_id| {
                (
                    (session.tool.clone(), session.project_id.clone()),
                    HashSet::from([external_session_id.clone()]),
                )
            })
        })
        .fold(
            HashMap::<(String, String), HashSet<String>>::new(),
            |mut assigned, (key, external_session_ids)| {
                assigned
                    .entry(key)
                    .or_default()
                    .extend(external_session_ids);
                assigned
            },
        );
    for session in &sessions {
        if !should_poll_runtime_session(session, reason, now_seconds()) {
            continue;
        }
        let mut request = probe_request_for_session(session);
        let assigned_key = (session.tool.clone(), session.project_id.clone());
        request.occupied_external_session_ids = assigned_external_session_ids
            .get(&assigned_key)
            .cloned()
            .unwrap_or_default();
        if let Some(external_session_id) = session.ai_session_id.as_ref() {
            request
                .occupied_external_session_ids
                .remove(external_session_id);
        }
        if let Some(snapshot) = probe_runtime_with_claude_cache(&request, claude_cache) {
            if let Some(external_session_id) = snapshot.external_session_id.clone() {
                assigned_external_session_ids
                    .entry(assigned_key)
                    .or_default()
                    .insert(external_session_id);
            }
            mutation.merge(state.apply_runtime_snapshot(&session.terminal_id, snapshot));
        }
        // Universal hook-free screen detection. Most tools only need this while
        // active (responding/needsInput). Kiro and CodeWhale also need it while
        // freshly idle: their persisted state can lag live generation, so the
        // rendered busy footer is the live-start signal.
        if matches!(session.state.as_str(), "responding" | "needsInput")
            || screen_starts_idle_tool(&session.tool)
        {
            mutation.merge(apply_screen_signal_for_session(state, registry, session));
        }
    }
    mutation
}

fn apply_screen_signal_for_terminal(
    state: &AIRuntimeStateStore,
    registry: &AIRuntimeRegistry,
    terminal_id: &str,
) -> AIRuntimeStateMutation {
    let sessions = state.sessions_for_terminals(&HashSet::from([terminal_id.to_string()]));
    let Some(session) = sessions.first() else {
        return AIRuntimeStateMutation::default();
    };
    apply_screen_signal_for_session(state, registry, session)
}

fn apply_screen_signal_for_session(
    state: &AIRuntimeStateStore,
    registry: &AIRuntimeRegistry,
    session: &crate::ai_runtime::AISessionSnapshot,
) -> AIRuntimeStateMutation {
    let signal = registry
        .screen_text(&session.terminal_id)
        .map(|text| detect_screen_signal(&text, &runtime_screen_patterns(&session.tool)))
        .unwrap_or(crate::ai_runtime::screen_signal::ScreenSignal::Unknown);
    state.apply_screen_signal(&session.terminal_id, signal)
}

fn start_poll_loop(tx: SyncSender<AIRuntimeSupervisorMessage>) {
    let _ = thread::Builder::new()
        .name("codux-ai-runtime-poller".to_string())
        .spawn(move || {
            loop {
                thread::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECONDS));
                if tx.send(AIRuntimeSupervisorMessage::Poll).is_err() {
                    break;
                }
            }
        });
}

fn start_transcript_monitor_loop(
    tx: SyncSender<AIRuntimeSupervisorMessage>,
    monitors: TranscriptMonitorMap,
) {
    let _ = thread::Builder::new()
        .name("codux-ai-runtime-transcript-monitor".to_string())
        .spawn(move || {
            loop {
                thread::sleep(std::time::Duration::from_millis(
                    TRANSCRIPT_MONITOR_INTERVAL_MS,
                ));
                // Periodic safety-net drain in case a filesystem event was
                // missed; the FS watcher handles the low-latency common case.
                if tx.send(AIRuntimeSupervisorMessage::DrainEventDir).is_err() {
                    return;
                }
                let changed = monitors
                    .lock()
                    .map(|mut monitors| {
                        if monitors.is_empty() {
                            Vec::new()
                        } else {
                            scan_transcript_monitors(&mut monitors, now_seconds())
                        }
                    })
                    .unwrap_or_default();
                if changed.is_empty() {
                    continue;
                }
                if tx
                    .send(AIRuntimeSupervisorMessage::TranscriptTail(changed))
                    .is_err()
                {
                    return;
                }
            }
        });
}

/// Deliver external CLI hook files with near-zero latency by watching the
/// runtime event directory instead of relying on the 3s fallback poll. Each
/// filesystem event nudges the supervisor to drain; draining itself stays on
/// the supervisor thread so reads/deletes never race. A dropped event simply
/// falls back to the next periodic drain.
fn start_event_dir_watcher(tx: SyncSender<AIRuntimeSupervisorMessage>, runtime_event_dir: PathBuf) {
    let _ = thread::Builder::new()
        .name("codux-ai-runtime-event-watcher".to_string())
        .spawn(move || {
            use notify::{EventKind, RecursiveMode, Watcher};

            if let Err(error) = std::fs::create_dir_all(&runtime_event_dir) {
                runtime_log_line(
                    "runtime-ingress",
                    &format!("event-watcher create-dir failed error={error}"),
                );
                return;
            }

            let nudge_tx = tx.clone();
            let mut watcher =
                match notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                    let Ok(event) = result else {
                        return;
                    };
                    if matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                        // Coalesce bursts: if a drain is already queued, dropping
                        // this nudge is harmless — the queued drain covers it.
                        let _ = nudge_tx.try_send(AIRuntimeSupervisorMessage::DrainEventDir);
                    }
                }) {
                    Ok(watcher) => watcher,
                    Err(error) => {
                        runtime_log_line(
                            "runtime-ingress",
                            &format!("event-watcher init failed error={error}"),
                        );
                        return;
                    }
                };

            if let Err(error) = watcher.watch(&runtime_event_dir, RecursiveMode::NonRecursive) {
                runtime_log_line(
                    "runtime-ingress",
                    &format!("event-watcher watch failed error={error}"),
                );
                return;
            }
            runtime_log_line(
                "runtime-ingress",
                &format!("event-watcher active dir={}", runtime_event_dir.display()),
            );

            // Drain anything already staged before the watch registered, then
            // park to keep the watcher (and its background thread) alive.
            let _ = tx.send(AIRuntimeSupervisorMessage::DrainEventDir);
            loop {
                thread::park();
            }
        });
}

fn start_binding_dir_watcher(tx: SyncSender<AIRuntimeSupervisorMessage>, binding_dir: PathBuf) {
    let _ = thread::Builder::new()
        .name("codux-ai-runtime-binding-watcher".to_string())
        .spawn(move || {
            use notify::{EventKind, RecursiveMode, Watcher};

            if let Err(error) = std::fs::create_dir_all(&binding_dir) {
                runtime_log_line(
                    "runtime-binding",
                    &format!("watcher create-dir failed error={error}"),
                );
                return;
            }

            let nudge_tx = tx.clone();
            let mut watcher =
                match notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                    let Ok(event) = result else {
                        return;
                    };
                    if matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                        for path in event.paths {
                            let Some(signature) = runtime_file_signature(&path) else {
                                continue;
                            };
                            let _ = nudge_tx.try_send(AIRuntimeSupervisorMessage::ScanBindingFile(
                                AIRuntimeBindingFileEvent { path, signature },
                            ));
                        }
                    }
                }) {
                    Ok(watcher) => watcher,
                    Err(error) => {
                        runtime_log_line(
                            "runtime-binding",
                            &format!("watcher init failed error={error}"),
                        );
                        return;
                    }
                };

            if let Err(error) = watcher.watch(&binding_dir, RecursiveMode::NonRecursive) {
                runtime_log_line(
                    "runtime-binding",
                    &format!("watcher watch failed error={error}"),
                );
                return;
            }
            runtime_log_line(
                "runtime-binding",
                &format!("watcher active dir={}", binding_dir.display()),
            );

            let _ = tx.send(AIRuntimeSupervisorMessage::ScanBindings);
            loop {
                thread::park();
            }
        });
}

// The agent host never drains this queue; drop the oldest past the cap so an
// undrained host stays bounded.
const MAX_PENDING_SUPERVISOR_EVENTS: usize = 256;

fn push_event(events: &Arc<Mutex<Vec<AIRuntimeSupervisorEvent>>>, event: AIRuntimeSupervisorEvent) {
    if let Ok(mut events) = events.lock() {
        if events.len() >= MAX_PENDING_SUPERVISOR_EVENTS {
            let overflow = events.len() + 1 - MAX_PENDING_SUPERVISOR_EVENTS;
            events.drain(..overflow);
        }
        events.push(event);
    }
}

// Process-tree detection is the hook-free bootstrap path: it creates the live
// terminal session that file probes then refine. It must be on by default, or a
// plain `codex` launched in a terminal has no current-session/loading state.
// Keep an explicit off switch for emergency diagnostics.
fn detection_enabled() -> bool {
    !std::env::var("CODUX_AI_RUNTIME_DETECT").is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "off" | "no"
        )
    })
}

fn now_seconds() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supervisor_applies_hook_frames_and_drains_events() {
        let supervisor = AIRuntimeSupervisor::new();
        let registry = AIRuntimeRegistry::shared();
        let dir = std::env::temp_dir().join(format!("codux-supervisor-{}", uuid::Uuid::new_v4()));
        let binding_dir = dir.join("bindings");
        supervisor
            .start(Arc::clone(&registry), dir.clone(), binding_dir)
            .unwrap();
        supervisor
            .submit_frame(
                br#"{"kind":"ai-hook","payload":{"kind":"promptSubmitted","terminalID":"term-1","projectID":"project-1","projectName":"Codux","projectPath":"/tmp/project","sessionTitle":"Task","tool":"codex","updatedAt":10}}"#
                    .to_vec(),
            )
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));

        let snapshot = supervisor.state_snapshot();
        assert_eq!(snapshot.running_count, 1);
        let events = supervisor.drain_events();
        assert!(matches!(
            events.first(),
            Some(AIRuntimeSupervisorEvent::RuntimeEvent { .. })
        ));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, AIRuntimeSupervisorEvent::State { .. }))
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn supervisor_emits_state_after_removing_session() {
        let supervisor = AIRuntimeSupervisor::new();
        let registry = AIRuntimeRegistry::shared();
        let dir = std::env::temp_dir().join(format!(
            "codux-supervisor-remove-session-{}",
            uuid::Uuid::new_v4()
        ));
        let binding_dir = dir.join("bindings");
        supervisor
            .start(registry, dir.clone(), binding_dir)
            .unwrap();
        supervisor
            .submit_frame(
                br#"{"kind":"ai-hook","payload":{"kind":"promptSubmitted","terminalID":"term-1","projectID":"project-1","projectName":"Codux","projectPath":"/tmp/project","sessionTitle":"Task","tool":"codex","updatedAt":10}}"#
                    .to_vec(),
            )
            .unwrap();
        wait_for(|| supervisor.state_snapshot().sessions.len() == 1);
        supervisor.drain_events();

        assert!(supervisor.remove_session("term-1"));
        wait_for(|| supervisor.state_snapshot().sessions.is_empty());
        assert!(!supervisor.remove_session("term-1"));

        let events = supervisor.drain_events();
        assert!(events.iter().any(|event| matches!(
            event,
            AIRuntimeSupervisorEvent::State { snapshot } if snapshot.sessions.is_empty()
        )));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn queued_hook_is_applied_before_session_removal() {
        let supervisor = AIRuntimeSupervisor::new();
        let registry = AIRuntimeRegistry::shared();
        let dir = std::env::temp_dir().join(format!(
            "codux-supervisor-remove-after-hook-{}",
            uuid::Uuid::new_v4()
        ));
        let binding_dir = dir.join("bindings");
        supervisor
            .start(registry, dir.clone(), binding_dir)
            .unwrap();

        supervisor
            .submit_frame(
                br#"{"kind":"ai-hook","payload":{"kind":"promptSubmitted","terminalID":"term-1","projectID":"project-1","projectName":"Codux","projectPath":"/tmp/project","sessionTitle":"Task","tool":"codex","updatedAt":10}}"#
                    .to_vec(),
            )
            .unwrap();
        assert!(supervisor.remove_session("term-1"));

        assert!(supervisor.state_snapshot().sessions.is_empty());
        let events = supervisor.drain_events();
        assert!(matches!(
            events.last(),
            Some(AIRuntimeSupervisorEvent::State { snapshot }) if snapshot.sessions.is_empty()
        ));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn supervisor_ignores_legacy_agy_hook_frames() {
        let supervisor = AIRuntimeSupervisor::new();
        let registry = AIRuntimeRegistry::shared();
        let dir =
            std::env::temp_dir().join(format!("codux-supervisor-agy-{}", uuid::Uuid::new_v4()));
        let binding_dir = dir.join("bindings");
        supervisor
            .start(Arc::clone(&registry), dir.clone(), binding_dir)
            .unwrap();
        supervisor
            .submit_frame(
                br#"{"kind":"ai-hook","payload":{"kind":"promptSubmitted","terminalID":"term-agy","projectID":"project-1","projectName":"Codux","projectPath":"/tmp/project","sessionTitle":"Agy","tool":"agy","updatedAt":10}}"#
                    .to_vec(),
            )
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(50));

        let snapshot = supervisor.state_snapshot();
        assert!(snapshot.sessions.is_empty());
        assert!(supervisor.drain_events().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn supervisor_drains_claude_hook_event_files() {
        let supervisor = AIRuntimeSupervisor::new();
        let registry = Arc::new(AIRuntimeRegistry::default());
        let dir = std::env::temp_dir().join(format!(
            "codux-supervisor-claude-events-{}",
            uuid::Uuid::new_v4()
        ));
        let binding_dir = dir.join("bindings");
        std::fs::create_dir_all(&dir).unwrap();
        let updated_at = now_seconds();
        std::fs::write(
            dir.join("100-claude-prompt.json"),
            format!(
                r#"{{"kind":"ai-hook","payload":{{"kind":"promptSubmitted","terminalID":"term-claude","terminalInstanceID":"instance-claude","projectID":"project-1","projectName":"Codux","projectPath":"/tmp/project","sessionTitle":"Claude","tool":"claude","aiSessionID":"external-claude","model":"sonnet","updatedAt":{updated_at},"metadata":{{"source":"user-input"}}}}}}"#
            ),
        )
        .unwrap();

        supervisor
            .start(registry, dir.clone(), binding_dir)
            .unwrap();
        wait_for(|| supervisor.state_snapshot().running_count == 1);

        let snapshot = supervisor.state_snapshot();
        assert_eq!(snapshot.running_count, 1);
        assert_eq!(snapshot.sessions[0].tool, "claude");
        assert_eq!(snapshot.sessions[0].terminal_id, "term-claude");
        assert_eq!(snapshot.sessions[0].state, "responding");
        assert!(!dir.join("100-claude-prompt.json").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn supervisor_applies_runtime_binding_files_without_process_detection() {
        let supervisor = AIRuntimeSupervisor::new();
        let registry = Arc::new(AIRuntimeRegistry::default());
        let dir =
            std::env::temp_dir().join(format!("codux-supervisor-binding-{}", uuid::Uuid::new_v4()));
        let binding_dir = dir.join("bindings");
        std::fs::create_dir_all(&binding_dir).unwrap();
        let started_at = now_seconds();
        std::fs::write(
            binding_dir.join("term-1-codex.json"),
            format!(
                r#"{{"runtimeBindingId":"instance-1-codex","terminalId":"term-1","terminalInstanceId":"instance-1","tool":"codex","projectId":"project-1","projectName":"Codux","projectPath":"/tmp/project","sessionTitle":"Codex","launchStartedAt":{started_at},"updatedAt":{started_at}}}"#
            ),
        )
        .unwrap();

        supervisor
            .start(registry, dir.clone(), binding_dir)
            .unwrap();
        wait_for(|| !supervisor.state_snapshot().sessions.is_empty());

        let snapshot = supervisor.state_snapshot();
        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.sessions[0].terminal_id, "term-1");
        assert_eq!(
            snapshot.sessions[0].terminal_instance_id.as_deref(),
            Some("instance-1")
        );
        assert_eq!(snapshot.sessions[0].tool, "codex");
        assert_eq!(snapshot.sessions[0].state, "idle");
        assert_eq!(snapshot.running_count, 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn binding_file_event_applies_only_changed_file() {
        let state = AIRuntimeStateStore::default();
        let binding_scan = Arc::new(Mutex::new(AIRuntimeBindingScanState::default()));
        let dir =
            std::env::temp_dir().join(format!("codux-binding-file-event-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("term-1-codex.json");
        std::fs::write(
            &path,
            r#"{"runtimeBindingId":"instance-1-codex","terminalId":"term-1","terminalInstanceId":"instance-1","tool":"codex","projectId":"project-1","projectName":"Codux","projectPath":"/tmp/project","sessionTitle":"Codex","launchStartedAt":1000.0,"updatedAt":1000.0}"#,
        )
        .unwrap();
        let signature = runtime_file_signature(&path).unwrap();

        let mutation = handle_binding_file_event(
            &state,
            &binding_scan,
            AIRuntimeBindingFileEvent {
                path: path.clone(),
                signature,
            },
        );
        assert!(mutation.did_change);
        let duplicate = handle_binding_file_event(
            &state,
            &binding_scan,
            AIRuntimeBindingFileEvent { path, signature },
        );
        assert!(!duplicate.did_change);

        let snapshot = state.snapshot();
        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.sessions[0].terminal_id, "term-1");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn supervisor_uses_terminal_registry_to_keep_running_session_live() {
        let supervisor = AIRuntimeSupervisor::new();
        let registry = AIRuntimeRegistry::shared();
        let dir = std::env::temp_dir().join(format!("codux-supervisor-{}", uuid::Uuid::new_v4()));
        let binding_dir = dir.join("bindings");
        let started_at = now_seconds();
        let prompt_at = started_at + 1.0;
        registry.upsert(crate::ai_runtime::registry::AIRuntimeTerminalBinding {
            root_project_id: Some("project-1".to_string()),
            worktree_id: Some("project-1".to_string()),
            terminal_id: "term-1".to_string(),
            project_id: "project-1".to_string(),
            slot_id: "slot-1".to_string(),
            title: "Task".to_string(),
            cwd: "/tmp/project".to_string(),
            tool: Some("codex".to_string()),
            is_active: true,
            session_key: Some("session-key-1".to_string()),
            terminal_instance_id: Some("instance-1".to_string()),
        });
        supervisor
            .start(Arc::clone(&registry), dir.clone(), binding_dir)
            .unwrap();
        supervisor
            .submit_frame(
                format!(
                    r#"{{"kind":"ai-hook","payload":{{"kind":"sessionStarted","terminalID":"term-1","terminalInstanceID":"instance-1","projectID":"project-1","projectName":"Codux","projectPath":"/tmp/project","sessionTitle":"Task","tool":"codex","aiSessionID":"session-1","updatedAt":{started_at}}}}}"#
                )
                .into_bytes(),
            )
            .unwrap();
        supervisor
            .submit_frame(
                format!(
                    r#"{{"kind":"ai-hook","payload":{{"kind":"promptSubmitted","terminalID":"term-1","terminalInstanceID":"instance-1","projectID":"project-1","projectName":"Codux","projectPath":"/tmp/project","sessionTitle":"Task","tool":"codex","aiSessionID":"session-1","updatedAt":{prompt_at}}}}}"#
                )
                .into_bytes(),
            )
            .unwrap();
        supervisor.poll_once().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));

        let snapshot = supervisor.state_snapshot();
        assert_eq!(snapshot.running_count, 1);
        assert_eq!(snapshot.completion_count, 0);
        assert_eq!(snapshot.sessions[0].state, "responding");
        assert!(!snapshot.sessions[0].was_interrupted);
        let terminals = registry.snapshot();
        assert_eq!(terminals.len(), 1);
        assert_eq!(terminals[0].terminal_id, "term-1");
        assert_eq!(
            terminals[0].terminal_instance_id.as_deref(),
            Some("instance-1")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn poll_ignores_codewhale_screen_signal_while_idle() {
        let state = AIRuntimeStateStore::default();
        let registry = AIRuntimeRegistry::shared();
        registry.upsert(crate::ai_runtime::registry::AIRuntimeTerminalBinding {
            root_project_id: Some("project-1".to_string()),
            worktree_id: Some("project-1".to_string()),
            terminal_id: "term-codewhale".to_string(),
            project_id: "project-1".to_string(),
            slot_id: "slot-1".to_string(),
            title: "CodeWhale".to_string(),
            cwd: "/tmp/codewhale-project".to_string(),
            tool: Some("codewhale".to_string()),
            is_active: true,
            session_key: Some("session-key-1".to_string()),
            terminal_instance_id: Some("instance-1".to_string()),
        });
        let screen = Arc::new(parking_lot::Mutex::new(
            codux_terminal_core::HeadlessTerminalScreen::new(80, 24, 100),
        ));
        screen
            .lock()
            .process(b"\xe2\x9c\xb6 Thinking... (3s \xc2\xb7 esc to interrupt)");
        registry.register_screen("term-codewhale", Arc::downgrade(&screen));
        state.apply_binding(crate::ai_runtime::binding::AIRuntimeBinding {
            runtime_binding_id: "instance-1-codewhale".to_string(),
            terminal_id: "term-codewhale".to_string(),
            terminal_instance_id: Some("instance-1".to_string()),
            tool: "codewhale".to_string(),
            project_id: "project-1".to_string(),
            project_name: "Codux".to_string(),
            project_path: Some("/tmp/codewhale-project".to_string()),
            session_title: "CodeWhale".to_string(),
            launch_started_at: now_seconds(),
            external_session_id: None,
            transcript_path: None,
            model: None,
            session_origin: None,
            updated_at: now_seconds(),
        });

        let mut claude_cache = ClaudeProbeCache::default();
        let mutation =
            poll_runtime_sessions(&state, &registry, "interval", None, &mut claude_cache);

        assert!(!mutation.did_change);
        let snapshot = state.snapshot();
        assert_eq!(snapshot.running_count, 0);
        assert_eq!(snapshot.sessions[0].tool, "codewhale");
        assert_eq!(snapshot.sessions[0].state, "idle");
    }

    #[test]
    fn process_detection_is_enabled_unless_explicitly_disabled() {
        unsafe {
            std::env::remove_var("CODUX_AI_RUNTIME_DETECT");
        }
        assert!(detection_enabled());
        unsafe {
            std::env::set_var("CODUX_AI_RUNTIME_DETECT", "0");
        }
        assert!(!detection_enabled());
        unsafe {
            std::env::set_var("CODUX_AI_RUNTIME_DETECT", "false");
        }
        assert!(!detection_enabled());
        unsafe {
            std::env::set_var("CODUX_AI_RUNTIME_DETECT", "1");
        }
        assert!(detection_enabled());
        unsafe {
            std::env::remove_var("CODUX_AI_RUNTIME_DETECT");
        }
    }

    fn wait_for(condition: impl Fn() -> bool) {
        let started = std::time::Instant::now();
        while started.elapsed() < std::time::Duration::from_secs(5) {
            if condition() {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}
