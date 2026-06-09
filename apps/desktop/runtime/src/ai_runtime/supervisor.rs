use crate::ai_runtime::{
    constants::{POLL_INTERVAL_SECONDS, TRANSCRIPT_MONITOR_INTERVAL_MS},
    event_file::drain_runtime_event_dir,
    frame::runtime_frame_to_hook,
    log::runtime_log_line,
    monitor::{TranscriptMonitorMap, refresh_transcript_monitors, scan_transcript_monitors},
    payload::AIRuntimeEvent,
    probe::probe_runtime,
    registry::AIRuntimeRegistry,
    snapshot::{AIRuntimeCompletionEvent, AIRuntimeStateSnapshot},
    store::{AIRuntimeStateMutation, AIRuntimeStateStore},
    store::{probe_request_for_session, should_poll_runtime_session},
};
use serde::Serialize;
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, SyncSender, sync_channel},
    },
    thread,
};

#[derive(Debug)]
enum AIRuntimeSupervisorMessage {
    HookFrame(Vec<u8>),
    Poll,
    TranscriptTail(Vec<String>),
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AIRuntimeSupervisorEvent {
    RuntimeEvent {
        event: AIRuntimeEvent,
    },
    State {
        snapshot: AIRuntimeStateSnapshot,
    },
    Completion {
        completion: AIRuntimeCompletionEvent,
    },
}

pub struct AIRuntimeSupervisor {
    tx: SyncSender<AIRuntimeSupervisorMessage>,
    rx: Mutex<Option<Receiver<AIRuntimeSupervisorMessage>>>,
    state: Arc<AIRuntimeStateStore>,
    transcript_monitors: TranscriptMonitorMap,
    events: Arc<Mutex<Vec<AIRuntimeSupervisorEvent>>>,
}

impl AIRuntimeSupervisor {
    pub fn new() -> Self {
        let (tx, rx) = sync_channel(1024);
        Self {
            tx,
            rx: Mutex::new(Some(rx)),
            state: Arc::new(AIRuntimeStateStore::default()),
            transcript_monitors: Arc::new(Mutex::new(Default::default())),
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn start(
        &self,
        registry: Arc<AIRuntimeRegistry>,
        runtime_event_dir: PathBuf,
    ) -> Result<(), String> {
        let mut receiver = self
            .rx
            .lock()
            .map_err(|_| "AI runtime supervisor lock poisoned.".to_string())?;
        let Some(rx) = receiver.take() else {
            return Ok(());
        };
        start_poll_loop(self.tx.clone());
        start_transcript_monitor_loop(
            self.tx.clone(),
            Arc::clone(&self.transcript_monitors),
            runtime_event_dir,
        );
        let state = Arc::clone(&self.state);
        let transcript_monitors = Arc::clone(&self.transcript_monitors);
        let events = Arc::clone(&self.events);
        thread::Builder::new()
            .name("codux-ai-runtime-supervisor".to_string())
            .spawn(move || supervisor_loop(rx, registry, state, transcript_monitors, events))
            .map_err(|error| error.to_string())?;
        Ok(())
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

    pub fn drain_events(&self) -> Vec<AIRuntimeSupervisorEvent> {
        self.events
            .lock()
            .map(|mut events| events.drain(..).collect())
            .unwrap_or_default()
    }
}

impl Default for AIRuntimeSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

fn supervisor_loop(
    rx: Receiver<AIRuntimeSupervisorMessage>,
    registry: Arc<AIRuntimeRegistry>,
    state: Arc<AIRuntimeStateStore>,
    transcript_monitors: TranscriptMonitorMap,
    events: Arc<Mutex<Vec<AIRuntimeSupervisorEvent>>>,
) {
    while let Ok(message) = rx.recv() {
        match message {
            AIRuntimeSupervisorMessage::HookFrame(frame) => {
                let Some(payload) = runtime_frame_to_hook(&frame) else {
                    runtime_log_line(
                        "runtime-ingress",
                        &format!("drop hook-frame reason=decode bytes={}", frame.len()),
                    );
                    continue;
                };
                runtime_log_line(
                    "runtime-ingress",
                    &format!(
                        "receive hook tool={} kind={} terminal={} project={}",
                        payload.tool, payload.kind, payload.terminal_id, payload.project_id
                    ),
                );
                push_event(
                    &events,
                    AIRuntimeSupervisorEvent::RuntimeEvent {
                        event: AIRuntimeEvent::Hook {
                            payload: payload.clone(),
                        },
                    },
                );
                let mutation = state.apply_hook(payload);
                runtime_log_line(
                    "runtime-ingress",
                    if mutation.did_change {
                        "apply hook result=changed"
                    } else {
                        "apply hook result=no-change"
                    },
                );
                after_mutation(&state, &transcript_monitors, &events, mutation);
            }
            AIRuntimeSupervisorMessage::Poll => {
                let mutation = poll_runtime_sessions(&state, &registry, "interval", None);
                after_mutation(&state, &transcript_monitors, &events, mutation);
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
                );
                after_mutation(&state, &transcript_monitors, &events, mutation);
            }
        }
    }
}

fn after_mutation(
    state: &AIRuntimeStateStore,
    transcript_monitors: &TranscriptMonitorMap,
    events: &Arc<Mutex<Vec<AIRuntimeSupervisorEvent>>>,
    mutation: AIRuntimeStateMutation,
) {
    refresh_transcript_monitors(transcript_monitors, &state.transcript_monitored_sessions());
    if mutation.did_change {
        push_event(
            events,
            AIRuntimeSupervisorEvent::State {
                snapshot: state.snapshot(),
            },
        );
    }
    if let Some(completion) = mutation.completion {
        push_event(events, AIRuntimeSupervisorEvent::Completion { completion });
    }
}

fn poll_runtime_sessions(
    state: &AIRuntimeStateStore,
    registry: &AIRuntimeRegistry,
    reason: &str,
    terminal_ids: Option<&HashSet<String>>,
) -> AIRuntimeStateMutation {
    let terminal_snapshot = registry.snapshot();
    let mut mutation = state.reconcile_bridge_snapshot(&terminal_snapshot);
    let now = now_seconds();
    let sessions = terminal_ids
        .map(|ids| state.sessions_for_terminals(ids))
        .unwrap_or_else(|| state.runtime_tracked_sessions(now));
    for session in sessions {
        if !should_poll_runtime_session(&session, reason, now_seconds()) {
            continue;
        }
        let request = probe_request_for_session(&session);
        if let Some(snapshot) = probe_runtime(&request) {
            mutation.merge(state.apply_runtime_snapshot(&session.terminal_id, snapshot));
        }
    }
    mutation
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
    runtime_event_dir: PathBuf,
) {
    let _ = thread::Builder::new()
        .name("codux-ai-runtime-transcript-monitor".to_string())
        .spawn(move || {
            loop {
                thread::sleep(std::time::Duration::from_millis(
                    TRANSCRIPT_MONITOR_INTERVAL_MS,
                ));
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
                for frame in drain_runtime_event_dir(&runtime_event_dir, now_seconds()) {
                    if tx
                        .send(AIRuntimeSupervisorMessage::HookFrame(frame))
                        .is_err()
                    {
                        return;
                    }
                }
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

fn push_event(events: &Arc<Mutex<Vec<AIRuntimeSupervisorEvent>>>, event: AIRuntimeSupervisorEvent) {
    if let Ok(mut events) = events.lock() {
        events.push(event);
    }
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
        supervisor
            .start(Arc::clone(&registry), dir.clone())
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
    fn supervisor_drains_claude_hook_event_files() {
        let supervisor = AIRuntimeSupervisor::new();
        let registry = Arc::new(AIRuntimeRegistry::default());
        let dir = std::env::temp_dir().join(format!(
            "codux-supervisor-claude-events-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let updated_at = now_seconds();
        std::fs::write(
            dir.join("100-claude-prompt.json"),
            format!(
                r#"{{"kind":"ai-hook","payload":{{"kind":"promptSubmitted","terminalID":"term-claude","terminalInstanceID":"instance-claude","projectID":"project-1","projectName":"Codux","projectPath":"/tmp/project","sessionTitle":"Claude","tool":"claude","aiSessionID":"external-claude","model":"sonnet","updatedAt":{updated_at},"metadata":{{"source":"user-input"}}}}}}"#
            ),
        )
        .unwrap();

        supervisor.start(registry, dir.clone()).unwrap();
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
    fn supervisor_uses_terminal_registry_to_keep_running_session_live() {
        let supervisor = AIRuntimeSupervisor::new();
        let registry = AIRuntimeRegistry::shared();
        let dir = std::env::temp_dir().join(format!("codux-supervisor-{}", uuid::Uuid::new_v4()));
        let started_at = now_seconds();
        let prompt_at = started_at + 1.0;
        registry.upsert(crate::ai_runtime::registry::AIRuntimeTerminalBinding {
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
            .start(Arc::clone(&registry), dir.clone())
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
    fn supervisor_creates_running_session_from_terminal_registry_without_hooks() {
        let supervisor = AIRuntimeSupervisor::new();
        let registry = Arc::new(AIRuntimeRegistry::default());
        let dir = std::env::temp_dir().join(format!(
            "codux-supervisor-registry-only-{}",
            uuid::Uuid::new_v4()
        ));
        registry.upsert(crate::ai_runtime::registry::AIRuntimeTerminalBinding {
            terminal_id: "term-1".to_string(),
            project_id: "project-1".to_string(),
            slot_id: "slot-1".to_string(),
            title: "Codex".to_string(),
            cwd: "/tmp/project".to_string(),
            tool: Some("codex".to_string()),
            is_active: true,
            session_key: Some("session-key-1".to_string()),
            terminal_instance_id: Some("instance-1".to_string()),
        });
        supervisor
            .start(Arc::clone(&registry), dir.clone())
            .unwrap();
        supervisor.poll_once().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));

        let snapshot = supervisor.state_snapshot();
        assert_eq!(snapshot.running_count, 1);
        assert_eq!(snapshot.sessions[0].terminal_id, "term-1");
        assert_eq!(snapshot.sessions[0].tool, "codex");
        assert_eq!(snapshot.sessions[0].state, "responding");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn supervisor_does_not_create_registry_session_without_active_session_key() {
        let supervisor = AIRuntimeSupervisor::new();
        let registry = Arc::new(AIRuntimeRegistry::default());
        let dir = std::env::temp_dir().join(format!(
            "codux-supervisor-registry-guard-{}",
            uuid::Uuid::new_v4()
        ));
        registry.upsert(crate::ai_runtime::registry::AIRuntimeTerminalBinding {
            terminal_id: "inactive-term".to_string(),
            project_id: "project-1".to_string(),
            slot_id: "slot-1".to_string(),
            title: "Codex".to_string(),
            cwd: "/tmp/project".to_string(),
            tool: Some("codex".to_string()),
            is_active: false,
            session_key: Some("session-key-1".to_string()),
            terminal_instance_id: Some("instance-1".to_string()),
        });
        registry.upsert(crate::ai_runtime::registry::AIRuntimeTerminalBinding {
            terminal_id: "missing-key-term".to_string(),
            project_id: "project-1".to_string(),
            slot_id: "slot-2".to_string(),
            title: "Codex".to_string(),
            cwd: "/tmp/project".to_string(),
            tool: Some("codex".to_string()),
            is_active: true,
            session_key: None,
            terminal_instance_id: Some("instance-2".to_string()),
        });
        supervisor
            .start(Arc::clone(&registry), dir.clone())
            .unwrap();
        supervisor.poll_once().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));

        let snapshot = supervisor.state_snapshot();
        assert_eq!(snapshot.running_count, 0);
        assert!(snapshot.sessions.is_empty());
        let _ = std::fs::remove_dir_all(dir);
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
