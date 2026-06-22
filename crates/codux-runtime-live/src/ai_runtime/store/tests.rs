use super::*;
use crate::ai_runtime::{AIHookEventMetadata, AIProjectPhase};
use std::fs;
use uuid::Uuid;

#[test]
fn hook_lifecycle_tracks_running_and_completion() {
    let store = AIRuntimeStateStore::default();
    let start = store.apply_hook(test_hook("promptSubmitted", 1000.0));
    assert!(start.did_change);
    assert!(start.completion.is_none());

    let snapshot = store.snapshot();
    assert_eq!(snapshot.running_count, 1);
    assert_eq!(snapshot.sessions[0].state, "responding");

    let complete = store.apply_hook(AIHookEventPayload {
        kind: "turnCompleted".to_string(),
        total_tokens: Some(150),
        updated_at: 1010.0,
        metadata: Some(AIHookEventMetadata {
            has_completed_turn: Some(true),
            ..empty_metadata()
        }),
        ..test_hook("turnCompleted", 1010.0)
    });

    assert!(complete.did_change);
    assert!(complete.completion.is_some());
    let snapshot = store.snapshot();
    assert_eq!(snapshot.running_count, 0);
    assert_eq!(snapshot.completion_count, 1);
    assert!(matches!(
        snapshot.projects[0].completed_phase,
        AIProjectPhase::Completed { .. }
    ));
}

#[test]
fn runtime_snapshot_sets_restored_session_baseline() {
    let mut core = AIRuntimeStateCore::default();
    assert!(apply_hook_unlocked(
        &mut core,
        test_hook("promptSubmitted", 1000.0)
    ));

    assert!(apply_runtime_snapshot_unlocked(
        &mut core,
        "terminal-1",
        AIRuntimeContextSnapshot {
            tool: "codex".to_string(),
            external_session_id: Some("session-1".to_string()),
            transcript_path: None,
            model: Some("gpt-5.5".to_string()),
            assistant_preview: None,
            input_tokens: 1_000,
            output_tokens: 200,
            cached_input_tokens: 3_000,
            total_tokens: 1_200,
            updated_at: 1005.0,
            started_at: Some(900.0),
            completed_at: None,
            response_state: Some("responding".to_string()),
            was_interrupted: false,
            has_completed_turn: false,
            session_origin: "restored".to_string(),
            source: "probe".to_string(),
            plan: None,
        }
    ));

    let session = core.sessions.get("terminal-1").unwrap();
    assert_eq!(session.baseline_total_tokens, 1_200);
    assert_eq!(session.baseline_cached_input_tokens, 3_000);
    assert!(session.baseline_resolved);
    assert_eq!(
        summary::project_totals_unlocked(&core, Some("project-1"), now_seconds()).total_tokens,
        0
    );
    assert_eq!(
        summary::project_totals_unlocked(&core, Some("project-1"), now_seconds())
            .cached_input_tokens,
        0
    );
}

#[test]
fn tool_activity_without_loading_is_ignored() {
    let store = AIRuntimeStateStore::default();
    let mut event = test_hook("promptSubmitted", 1000.0);
    event.metadata = Some(AIHookEventMetadata {
        source: Some("tool-use".to_string()),
        ..empty_metadata()
    });

    let mutation = store.apply_hook(event);

    assert!(!mutation.did_change);
    assert!(store.snapshot().sessions.is_empty());
}

#[test]
fn codewhale_hook_is_tracked_as_runtime_session() {
    let store = AIRuntimeStateStore::default();
    let mutation = store.apply_hook(test_hook_for(
        "deepseek-tui",
        "codewhale-term-1",
        "codewhale-session-1",
        1000.0,
    ));

    assert!(mutation.did_change);
    let snapshot = store.snapshot();
    assert_eq!(snapshot.running_count, 1);
    assert_eq!(snapshot.sessions[0].tool, "codewhale");
    assert_eq!(snapshot.sessions[0].terminal_id, "codewhale-term-1");
    assert_eq!(
        snapshot.sessions[0].ai_session_id.as_deref(),
        Some("codewhale-session-1")
    );
}

#[test]
fn codewhale_terminal_bridge_is_not_filtered() {
    let terminal = AIRuntimeTerminalState {
        terminal_id: "codewhale-term-1".to_string(),
        project_id: "project-1".to_string(),
        slot_id: "slot-1".to_string(),
        title: "CodeWhale".to_string(),
        cwd: "/tmp/codewhale-project".to_string(),
        tool: Some("codewhale-tui".to_string()),
        is_active: true,
        session_key: Some("codewhale-session-1".to_string()),
        terminal_instance_id: Some("instance-1".to_string()),
    };

    let session = bridge_terminal_session(&terminal, 1000.0).expect("session");

    assert_eq!(session.tool, "codewhale");
    assert_eq!(session.terminal_id, "codewhale-term-1");
    assert_eq!(
        session.ai_session_id.as_deref(),
        Some("codewhale-session-1")
    );
}

#[test]
fn codewhale_completion_merges_realtime_probe_snapshot() {
    let root = std::env::temp_dir().join(format!("codux-codewhale-store-probe-{}", Uuid::new_v4()));
    let project = root.join("project");
    let session_dir = root.join(".codewhale").join("sessions");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&session_dir).unwrap();
    let session_file = session_dir.join("session-1.json");
    fs::write(
        &session_file,
        format!(
            r#"{{
                "metadata": {{
                    "id": "session-1",
                    "workspace": "{}",
                    "model": "deepseek-chat",
                    "total_tokens": 789,
                    "created_at": "2026-06-06T01:00:00Z",
                    "updated_at": "2026-06-06T01:01:00Z"
                }},
                "messages": [
                    {{ "role": "assistant", "content": "done" }}
                ]
            }}"#,
            project.display()
        ),
    )
    .unwrap();
    let store = AIRuntimeStateStore::default();
    let mut prompt = test_hook_for("codewhale", "codewhale-term-1", "session-1", 1000.0);
    prompt.project_path = Some(project.display().to_string());
    prompt.model = None;
    assert!(store.apply_hook(prompt).did_change);

    let mut complete = test_hook_for("deepseek-tui", "codewhale-term-1", "session-1", 1010.0);
    complete.kind = "turnCompleted".to_string();
    complete.project_path = Some(project.display().to_string());
    complete.model = None;
    complete.metadata = Some(AIHookEventMetadata {
        transcript_path: Some(session_file.display().to_string()),
        has_completed_turn: Some(true),
        ..empty_metadata()
    });
    assert!(store.apply_hook(complete).did_change);

    let snapshot = store.snapshot();
    let session = snapshot
        .sessions
        .iter()
        .find(|session| session.terminal_id == "codewhale-term-1")
        .unwrap();
    assert_eq!(session.tool, "codewhale");
    assert_eq!(session.model.as_deref(), Some("deepseek-chat"));
    assert_eq!(session.total_tokens, 789);
    assert_eq!(session.state, "idle");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn codex_stale_completed_turn_after_new_prompt_stays_running() {
    let mut core = AIRuntimeStateCore::default();
    assert!(apply_hook_unlocked(
        &mut core,
        test_hook("promptSubmitted", 1000.0)
    ));
    assert!(apply_hook_unlocked(
        &mut core,
        AIHookEventPayload {
            kind: "turnCompleted".to_string(),
            updated_at: 1010.0,
            metadata: Some(AIHookEventMetadata {
                has_completed_turn: Some(true),
                ..empty_metadata()
            }),
            ..test_hook("turnCompleted", 1010.0)
        }
    ));
    assert!(apply_hook_unlocked(
        &mut core,
        test_hook("promptSubmitted", 1020.0)
    ));
    let previous = core.sessions.get("terminal-1").cloned().unwrap();

    let resolved = merge_snapshot_into_hook(
        AIHookEventPayload {
            kind: "turnCompleted".to_string(),
            updated_at: 1021.0,
            metadata: Some(AIHookEventMetadata {
                transcript_path: Some("/tmp/codex.jsonl".to_string()),
                ..empty_metadata()
            }),
            ..test_hook("turnCompleted", 1021.0)
        },
        AIRuntimeContextSnapshot {
            tool: "codex".to_string(),
            external_session_id: Some("session-1".to_string()),
            transcript_path: Some("/tmp/codex.jsonl".to_string()),
            model: Some("gpt-5.4".to_string()),
            assistant_preview: None,
            input_tokens: 0,
            output_tokens: 0,
            cached_input_tokens: 0,
            total_tokens: 150,
            updated_at: 1010.0,
            started_at: Some(1000.0),
            completed_at: Some(1010.0),
            response_state: Some("idle".to_string()),
            was_interrupted: false,
            has_completed_turn: true,
            session_origin: "live".to_string(),
            source: "probe".to_string(),
            plan: None,
        },
        Some(&previous),
    );

    assert_eq!(resolved.kind, "promptSubmitted");
    assert_eq!(
        resolved
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.has_completed_turn),
        Some(false)
    );
    assert!(apply_hook_unlocked(&mut core, resolved));
    let session = core.sessions.get("terminal-1").unwrap();
    assert_eq!(session.state, "responding");
    assert!(session.has_completed_turn);
    assert!(matches!(
        completed_phase_unlocked(&core, "project-1", now_seconds()),
        AIProjectPhase::Idle
    ));
}

#[test]
fn stale_session_started_does_not_override_running_prompt() {
    let mut core = AIRuntimeStateCore::default();
    assert!(apply_hook_unlocked(
        &mut core,
        test_hook("promptSubmitted", 1000.0)
    ));

    assert!(!apply_hook_unlocked(
        &mut core,
        test_hook("sessionStarted", 999.0)
    ));

    let session = core.sessions.get("terminal-1").unwrap();
    assert_eq!(session.state, "responding");
    assert_eq!(session.updated_at, 1000.0);
}

#[test]
fn session_started_clears_previous_completion_flag() {
    let mut core = AIRuntimeStateCore::default();
    assert!(apply_hook_unlocked(
        &mut core,
        test_hook("promptSubmitted", 1000.0)
    ));
    assert!(apply_hook_unlocked(
        &mut core,
        AIHookEventPayload {
            kind: "turnCompleted".to_string(),
            updated_at: 1010.0,
            metadata: Some(AIHookEventMetadata {
                has_completed_turn: Some(true),
                ..empty_metadata()
            }),
            ..test_hook("turnCompleted", 1010.0)
        }
    ));
    assert!(apply_hook_unlocked(
        &mut core,
        test_hook("sessionStarted", 1020.0)
    ));

    let session = core.sessions.get("terminal-1").unwrap();
    assert_eq!(session.state, "idle");
    assert!(!session.has_completed_turn);
    assert!(matches!(
        completed_phase_unlocked(&core, "project-1", now_seconds()),
        AIProjectPhase::Idle
    ));
}

#[test]
fn stale_needs_input_is_not_visible_in_snapshot() {
    let mut core = AIRuntimeStateCore::default();
    assert!(apply_hook_unlocked(
        &mut core,
        AIHookEventPayload {
            kind: "needsInput".to_string(),
            updated_at: 1.0,
            metadata: Some(AIHookEventMetadata {
                notification_type: Some("permission-request".to_string()),
                target_tool_name: Some("AskUserQuestion".to_string()),
                ..empty_metadata()
            }),
            ..test_hook_for("claude", "claude-term-1", "claude-external-1", 1.0)
        }
    ));

    let snapshot = state_snapshot_unlocked(&core);

    assert_eq!(snapshot.needs_input_count, 0);
    assert_eq!(snapshot.global_totals.needs_input, 0);
    assert_eq!(snapshot.sessions[0].state, "idle");
    assert!(snapshot.sessions[0].notification_type.is_none());
    assert!(matches!(
        snapshot.projects[0].project_phase,
        AIProjectPhase::Idle
    ));
}

#[test]
fn fresh_needs_input_remains_visible_in_snapshot() {
    let now = now_seconds();
    let mut core = AIRuntimeStateCore::default();
    assert!(apply_hook_unlocked(
        &mut core,
        AIHookEventPayload {
            kind: "needsInput".to_string(),
            updated_at: now,
            metadata: Some(AIHookEventMetadata {
                notification_type: Some("permission-request".to_string()),
                target_tool_name: Some("AskUserQuestion".to_string()),
                ..empty_metadata()
            }),
            ..test_hook_for("claude", "claude-term-1", "claude-external-1", now)
        }
    ));

    let snapshot = state_snapshot_unlocked(&core);

    assert_eq!(snapshot.needs_input_count, 1);
    assert_eq!(snapshot.global_totals.needs_input, 1);
    assert_eq!(snapshot.sessions[0].state, "needsInput");
    assert!(matches!(
        snapshot.projects[0].project_phase,
        AIProjectPhase::NeedsInput { .. }
    ));
}

#[test]
fn prompt_submitted_after_needs_input_restores_running_state() {
    let mut core = AIRuntimeStateCore::default();
    assert!(apply_hook_unlocked(
        &mut core,
        AIHookEventPayload {
            kind: "needsInput".to_string(),
            updated_at: 1000.0,
            metadata: Some(AIHookEventMetadata {
                notification_type: Some("permission-request".to_string()),
                target_tool_name: Some("AskUserQuestion".to_string()),
                ..empty_metadata()
            }),
            ..test_hook_for("claude", "claude-term-1", "claude-external-1", 1000.0)
        }
    ));
    assert!(apply_hook_unlocked(
        &mut core,
        AIHookEventPayload {
            kind: "promptSubmitted".to_string(),
            updated_at: 1001.0,
            metadata: Some(AIHookEventMetadata {
                source: Some("permission-auto-allowed".to_string()),
                ..empty_metadata()
            }),
            ..test_hook_for("claude", "claude-term-1", "claude-external-1", 1001.0)
        }
    ));

    let snapshot = state_snapshot_unlocked(&core);

    assert_eq!(snapshot.running_count, 1);
    assert_eq!(snapshot.needs_input_count, 0);
    assert_eq!(snapshot.sessions[0].state, "responding");
    assert!(snapshot.sessions[0].notification_type.is_none());
}

#[test]
fn claude_needs_input_clears_when_probe_sees_resume_after_completed_turn() {
    // Repro of the desktop pet sticking on "等待允许" after a manual permission
    // approval: a prior turn completed (has_completed_turn=true), the user sends
    // a new prompt, Claude asks for permission (needsInput). On approval no hook
    // fires (Claude has no "granted" hook), so the 5s probe is what must clear
    // it once Claude resumes responding.
    let mut core = AIRuntimeStateCore::default();
    assert!(apply_hook_unlocked(
        &mut core,
        test_hook_for("claude", "claude-term-1", "claude-external-1", 1000.0)
    ));
    assert!(apply_hook_unlocked(
        &mut core,
        AIHookEventPayload {
            kind: "turnCompleted".to_string(),
            updated_at: 1010.0,
            metadata: Some(AIHookEventMetadata {
                has_completed_turn: Some(true),
                ..empty_metadata()
            }),
            ..test_hook_for("claude", "claude-term-1", "claude-external-1", 1010.0)
        }
    ));
    assert!(apply_hook_unlocked(
        &mut core,
        test_hook_for("claude", "claude-term-1", "claude-external-1", 1020.0)
    ));
    assert!(apply_hook_unlocked(
        &mut core,
        AIHookEventPayload {
            kind: "needsInput".to_string(),
            updated_at: 1025.0,
            metadata: Some(AIHookEventMetadata {
                notification_type: Some("permission-request".to_string()),
                target_tool_name: Some("Skill".to_string()),
                ..empty_metadata()
            }),
            ..test_hook_for("claude", "claude-term-1", "claude-external-1", 1025.0)
        }
    ));
    assert_eq!(core.sessions.get("claude-term-1").unwrap().state, "needsInput");

    // User approves; Claude resumes. The probe reads the log: the turn is still
    // responding (last user prompt newer than last completion) and new output
    // advanced `updated_at`.
    apply_runtime_snapshot_unlocked(
        &mut core,
        "claude-term-1",
        AIRuntimeContextSnapshot {
            tool: "claude".to_string(),
            external_session_id: Some("claude-external-1".to_string()),
            transcript_path: None,
            model: Some("sonnet".to_string()),
            assistant_preview: None,
            input_tokens: 0,
            output_tokens: 0,
            cached_input_tokens: 0,
            total_tokens: 200,
            updated_at: 1030.0,
            // Log's user-message time sits slightly before the hook's prompt
            // wall-clock time (real-world skew between the two clocks).
            started_at: Some(1018.0),
            completed_at: Some(1010.0),
            response_state: Some("responding".to_string()),
            was_interrupted: false,
            has_completed_turn: false,
            session_origin: "live".to_string(),
            source: "probe".to_string(),
            plan: None,
        },
    );

    assert_eq!(
        core.sessions.get("claude-term-1").unwrap().state,
        "responding",
        "needsInput must clear once the probe sees the turn resume after approval"
    );
}

#[test]
fn prompt_submitted_clears_previous_interruption_flag() {
    let mut core = AIRuntimeStateCore::default();
    assert!(apply_hook_unlocked(
        &mut core,
        test_hook("promptSubmitted", 1000.0)
    ));
    assert!(apply_hook_unlocked(
        &mut core,
        AIHookEventPayload {
            kind: "turnCompleted".to_string(),
            updated_at: 1010.0,
            metadata: Some(AIHookEventMetadata {
                was_interrupted: Some(true),
                has_completed_turn: Some(false),
                ..empty_metadata()
            }),
            ..test_hook("turnCompleted", 1010.0)
        }
    ));
    assert!(apply_hook_unlocked(
        &mut core,
        test_hook("promptSubmitted", 1020.0)
    ));

    let session = core.sessions.get("terminal-1").unwrap();
    assert_eq!(session.state, "responding");
    assert!(!session.was_interrupted);
    assert!(!session.has_completed_turn);
}

#[test]
fn reconcile_without_live_terminal_silently_retires_running_session() {
    // A turn whose terminal has vanished is retired silently: loading stops but
    // it must NOT masquerade as an interruption or completion, so no
    // notification fires and it is not enqueued for memory extraction.
    let store = AIRuntimeStateStore::default();
    assert!(
        store
            .apply_hook(test_hook("promptSubmitted", 1000.0))
            .did_change
    );

    let mutation = store.reconcile_bridge_snapshot(&[]);

    assert!(mutation.did_change);
    assert!(mutation.completion.is_none());
    let snapshot = store.snapshot();
    assert_eq!(snapshot.running_count, 0);
    assert_eq!(snapshot.completion_count, 0);
    assert_eq!(snapshot.sessions[0].state, "idle");
    assert!(!snapshot.sessions[0].was_interrupted);
    assert!(!snapshot.sessions[0].has_completed_turn);
}

#[test]
fn reconcile_prunes_orphaned_idle_session_after_retention() {
    let store = AIRuntimeStateStore::default();
    assert!(
        store
            .apply_hook(test_hook("promptSubmitted", 1000.0))
            .did_change
    );
    {
        let mut core = store.core.lock().unwrap();
        let session = core.sessions.get_mut("terminal-1").unwrap();
        session.state = "idle".to_string();
        session.updated_at = now_seconds()
            - crate::ai_runtime::constants::IDLE_SESSION_RETENTION_SECONDS
            - 10.0;
    }
    // Terminal is gone and the idle session is well past retention -> reclaimed.
    let mutation = store.reconcile_bridge_snapshot(&[]);
    assert!(mutation.did_change);
    assert!(store.snapshot().sessions.is_empty());

    // A recently-idled orphan is still retained (within the window).
    assert!(
        store
            .apply_hook(test_hook("promptSubmitted", now_seconds()))
            .did_change
    );
    store.reconcile_bridge_snapshot(&[]);
    assert_eq!(store.snapshot().sessions.len(), 1);
}

fn responding_probe_snapshot(updated_at: f64) -> AIRuntimeContextSnapshot {
    AIRuntimeContextSnapshot {
        tool: "codex".to_string(),
        external_session_id: Some("session-1".to_string()),
        transcript_path: None,
        model: Some("gpt-5.5".to_string()),
        assistant_preview: None,
        input_tokens: 0,
        output_tokens: 0,
        cached_input_tokens: 0,
        total_tokens: 0,
        updated_at,
        started_at: None,
        completed_at: None,
        response_state: Some("responding".to_string()),
        was_interrupted: false,
        has_completed_turn: false,
        session_origin: "live".to_string(),
        source: "probe".to_string(),
        plan: None,
    }
}

fn codex_bridge_terminal() -> crate::ai_runtime::registry::AIRuntimeTerminalState {
    crate::ai_runtime::registry::AIRuntimeTerminalState {
        terminal_id: "terminal-1".to_string(),
        terminal_instance_id: Some("instance-1".to_string()),
        project_id: "project-1".to_string(),
        slot_id: "slot-1".to_string(),
        title: "Codex".to_string(),
        cwd: "/tmp/codex-project".to_string(),
        tool: Some("codex".to_string()),
        is_active: true,
        session_key: Some("session-1".to_string()),
    }
}

#[test]
fn responding_heartbeat_stops_renewing_after_turn_exceeds_ceiling() {
    // A turn that started long ago and whose transcript still merely *parses* as
    // "responding" (e.g. the CLI was killed mid-turn while the terminal tab
    // stayed open) must not have its heartbeat synthesized forever, otherwise
    // reconcile never ages it and the pet bubble stays pinned.
    let mut core = AIRuntimeStateCore::default();
    assert!(apply_hook_unlocked(
        &mut core,
        test_hook("promptSubmitted", 1000.0)
    ));
    assert_eq!(core.sessions.get("terminal-1").unwrap().state, "responding");

    // The probe keeps reporting "responding" with no genuine transcript
    // progress (snapshot.updated_at stays in the distant past).
    apply_runtime_snapshot_unlocked(&mut core, "terminal-1", responding_probe_snapshot(1000.0));

    // Because the turn is older than the renewal ceiling, updated_at is NOT
    // pulled forward to "now" — it stays anchored in the past so staleness aging
    // can fire.
    let session = core.sessions.get("terminal-1").unwrap();
    assert!(
        session.updated_at
            < now_seconds() - crate::ai_runtime::constants::RESPONDING_RENEWAL_MAX_SECONDS,
        "stale responding turn should not renew its heartbeat (updated_at={})",
        session.updated_at
    );

    // reconcile, with the terminal still live, now sees a stale responding
    // session and silently retires it -> idle, releasing the pet bubble without
    // firing a spurious "interrupted" notification.
    let store = AIRuntimeStateStore::default();
    *store.core.lock().unwrap() = core;
    let result = store.reconcile_bridge_snapshot(&[codex_bridge_terminal()]);
    assert!(result.did_change);
    assert!(result.completion.is_none());
    let snapshot = store.snapshot();
    assert_eq!(snapshot.running_count, 0);
    assert_eq!(snapshot.sessions[0].state, "idle");
    assert!(!snapshot.sessions[0].was_interrupted);
}

#[test]
fn remove_session_drops_closed_terminal_from_snapshot() {
    // Closing a terminal tab must evict its session from the live state so it
    // stops appearing in the current-session aggregate (otherwise stale cards
    // linger after the tab is gone).
    let store = AIRuntimeStateStore::default();
    assert!(store.reconcile_bridge_snapshot(&[codex_bridge_terminal()]).did_change);
    assert_eq!(store.snapshot().sessions.len(), 1);

    assert!(store.remove_session("terminal-1"));
    assert!(store.snapshot().sessions.is_empty());

    // Removing an unknown terminal is a no-op.
    assert!(!store.remove_session("terminal-1"));
}

#[test]
fn note_output_activity_only_sustains_a_responding_turn() {
    let store = AIRuntimeStateStore::default();
    let mut core = AIRuntimeStateCore::default();
    assert!(apply_hook_unlocked(
        &mut core,
        test_hook("promptSubmitted", 1000.0)
    ));
    assert_eq!(core.sessions.get("terminal-1").unwrap().state, "responding");
    *store.core.lock().unwrap() = core;

    // Real output during a responding turn pulls updated_at forward.
    let now = now_seconds();
    assert!(store.note_output_activity("terminal-1", now));
    assert!(store.core.lock().unwrap().sessions["terminal-1"].updated_at >= now - 1.0);

    // Unknown terminals never create a session (service/shell output is inert).
    assert!(!store.note_output_activity("terminal-unknown", now));

    // Once the turn goes idle, output no longer fabricates activity.
    store.core.lock().unwrap().sessions.get_mut("terminal-1").unwrap().state = "idle".to_string();
    assert!(!store.note_output_activity("terminal-1", now + 10.0));
}

#[test]
fn responding_heartbeat_renews_within_ceiling_across_quiet_gap() {
    // A genuinely active turn that has only just gone quiet (well within the
    // ceiling) must still have its heartbeat renewed so it is not interrupted
    // mid-flight.
    let now = now_seconds();
    let mut core = AIRuntimeStateCore::default();
    assert!(apply_hook_unlocked(
        &mut core,
        test_hook("promptSubmitted", now - 60.0)
    ));
    assert_eq!(core.sessions.get("terminal-1").unwrap().state, "responding");

    apply_runtime_snapshot_unlocked(
        &mut core,
        "terminal-1",
        responding_probe_snapshot(now - 60.0),
    );

    let session = core.sessions.get("terminal-1").unwrap();
    assert_eq!(session.state, "responding");
    assert!(
        session.updated_at >= now - 5.0,
        "fresh responding turn should renew its heartbeat to ~now (updated_at={}, now={now})",
        session.updated_at
    );
}

#[test]
fn first_prompt_notifies_when_bridge_already_marked_terminal_running() {
    let store = AIRuntimeStateStore::default();
    let bridge = crate::ai_runtime::registry::AIRuntimeTerminalState {
        terminal_id: "terminal-1".to_string(),
        terminal_instance_id: Some("instance-1".to_string()),
        project_id: "project-1".to_string(),
        slot_id: "slot-1".to_string(),
        title: "Codex".to_string(),
        cwd: "/tmp/codex-project".to_string(),
        tool: Some("codex".to_string()),
        is_active: true,
        session_key: Some("session-1".to_string()),
    };

    assert!(store.reconcile_bridge_snapshot(&[bridge]).did_change);

    let prompt = test_hook("promptSubmitted", 1000.0);
    let mutation = store.apply_hook(prompt);

    assert!(mutation.did_change);
    assert!(mutation.completion.is_none());
    let snapshot = store.snapshot();
    assert_eq!(snapshot.running_count, 1);
    assert_eq!(snapshot.sessions[0].state, "responding");
}

#[test]
fn prompt_submitted_uses_wrapper_project_even_when_hook_cwd_differs() {
    let store = AIRuntimeStateStore::default();
    let mut prompt = test_hook("promptSubmitted", 1000.0);
    prompt.project_path = Some("F:\\codux-gpui".to_string());
    prompt.metadata = Some(AIHookEventMetadata {
        cwd: Some("C:\\Users\\dux".to_string()),
        ..empty_metadata()
    });

    let mutation = store.apply_hook(prompt);

    assert!(mutation.did_change);
    let snapshot = store.snapshot();
    assert_eq!(snapshot.running_count, 1);
    assert_eq!(snapshot.sessions[0].state, "responding");
}

#[test]
fn multiple_same_tool_sessions_are_isolated_by_terminal_id() {
    let store = AIRuntimeStateStore::default();
    assert!(
        store
            .apply_hook(test_hook_for(
                "codex",
                "codex-term-1",
                "codex-session-1",
                1000.0
            ))
            .did_change
    );
    assert!(
        store
            .apply_hook(test_hook_for(
                "codex",
                "codex-term-2",
                "codex-session-2",
                1001.0
            ))
            .did_change
    );

    let snapshot = store.snapshot();
    assert_eq!(snapshot.running_count, 1);
    assert_eq!(snapshot.global_totals.running, 2);
    assert!(
        snapshot
            .sessions
            .iter()
            .any(|session| session.terminal_id == "codex-term-1"
                && session.ai_session_id.as_deref() == Some("codex-session-1")
                && session.state == "responding")
    );
    assert!(
        snapshot
            .sessions
            .iter()
            .any(|session| session.terminal_id == "codex-term-2"
                && session.ai_session_id.as_deref() == Some("codex-session-2")
                && session.state == "responding")
    );

    assert!(
        store
            .apply_hook(AIHookEventPayload {
                kind: "turnCompleted".to_string(),
                updated_at: 1010.0,
                metadata: Some(AIHookEventMetadata {
                    has_completed_turn: Some(true),
                    ..empty_metadata()
                }),
                ..test_hook_for("codex", "codex-term-1", "codex-session-1", 1010.0)
            })
            .did_change
    );

    let snapshot = store.snapshot();
    assert_eq!(snapshot.running_count, 1);
    assert_eq!(snapshot.global_totals.running, 1);
    assert!(
        snapshot
            .sessions
            .iter()
            .any(|session| session.terminal_id == "codex-term-1" && session.state == "idle")
    );
    assert!(
        snapshot
            .sessions
            .iter()
            .any(|session| session.terminal_id == "codex-term-2"
                && session.ai_session_id.as_deref() == Some("codex-session-2")
                && session.state == "responding")
    );
}

#[test]
fn multiple_claude_sessions_are_isolated_by_terminal_id_and_external_session_id() {
    let store = AIRuntimeStateStore::default();
    assert!(
        store
            .apply_hook(test_hook_for(
                "claude",
                "claude-term-1",
                "claude-external-1",
                1000.0
            ))
            .did_change
    );
    assert!(
        store
            .apply_hook(test_hook_for(
                "claude",
                "claude-term-2",
                "claude-external-2",
                1001.0
            ))
            .did_change
    );

    let snapshot = store.snapshot();
    assert_eq!(snapshot.running_count, 1);
    assert_eq!(snapshot.global_totals.running, 2);
    assert!(
        snapshot
            .sessions
            .iter()
            .any(|session| session.terminal_id == "claude-term-1"
                && session.tool == "claude"
                && session.ai_session_id.as_deref() == Some("claude-external-1"))
    );
    assert!(
        snapshot
            .sessions
            .iter()
            .any(|session| session.terminal_id == "claude-term-2"
                && session.tool == "claude"
                && session.ai_session_id.as_deref() == Some("claude-external-2"))
    );
}

#[test]
fn stale_runtime_completion_snapshot_after_prompt_stays_running() {
    let store = AIRuntimeStateStore::default();
    assert!(
        store
            .apply_hook(test_hook("sessionStarted", 1000.0))
            .did_change
    );
    assert!(
        store
            .apply_hook(test_hook("promptSubmitted", 1020.0))
            .did_change
    );

    let mutation = store.apply_runtime_snapshot(
        "terminal-1",
        AIRuntimeContextSnapshot {
            tool: "codex".to_string(),
            external_session_id: Some("session-1".to_string()),
            transcript_path: Some("/tmp/codex.jsonl".to_string()),
            model: Some("gpt-5.4".to_string()),
            assistant_preview: None,
            input_tokens: 0,
            output_tokens: 0,
            cached_input_tokens: 0,
            total_tokens: 150,
            updated_at: 1010.0,
            started_at: Some(1000.0),
            completed_at: Some(1010.0),
            response_state: Some("idle".to_string()),
            was_interrupted: false,
            has_completed_turn: true,
            session_origin: "live".to_string(),
            source: "probe".to_string(),
            plan: None,
        },
    );

    assert!(mutation.did_change);
    assert!(mutation.completion.is_none());
    let snapshot = store.snapshot();
    assert_eq!(snapshot.running_count, 1);
    assert_eq!(snapshot.completion_count, 0);
    assert_eq!(snapshot.sessions[0].state, "responding");
    assert!(!snapshot.sessions[0].has_completed_turn);
    assert!(matches!(
        snapshot.projects[0].completed_phase,
        AIProjectPhase::Idle
    ));
}

#[test]
fn same_second_completion_snapshot_after_prompt_completes() {
    let store = AIRuntimeStateStore::default();
    assert!(
        store
            .apply_hook(test_hook("sessionStarted", 1000.0))
            .did_change
    );
    assert!(
        store
            .apply_hook(test_hook("promptSubmitted", 1020.178))
            .did_change
    );

    let mutation = store.apply_runtime_snapshot(
        "terminal-1",
        AIRuntimeContextSnapshot {
            tool: "codex".to_string(),
            external_session_id: Some("session-1".to_string()),
            transcript_path: Some("/tmp/codex.jsonl".to_string()),
            model: Some("gpt-5.4".to_string()),
            assistant_preview: None,
            input_tokens: 0,
            output_tokens: 0,
            cached_input_tokens: 0,
            total_tokens: 150,
            updated_at: 1020.743,
            started_at: Some(1000.0),
            completed_at: Some(1020.0),
            response_state: Some("idle".to_string()),
            was_interrupted: false,
            has_completed_turn: true,
            session_origin: "live".to_string(),
            source: "probe".to_string(),
            plan: None,
        },
    );

    assert!(mutation.did_change);
    assert!(mutation.completion.is_some());
    let snapshot = store.snapshot();
    assert_eq!(snapshot.running_count, 0);
    assert_eq!(snapshot.completion_count, 1);
    assert_eq!(snapshot.sessions[0].state, "idle");
    assert!(snapshot.sessions[0].has_completed_turn);
}

#[test]
fn later_probe_for_same_completed_turn_does_not_notify_twice() {
    let store = AIRuntimeStateStore::default();
    assert!(
        store
            .apply_hook(test_hook("sessionStarted", 1000.0))
            .did_change
    );
    assert!(
        store
            .apply_hook(test_hook("promptSubmitted", 1020.0))
            .did_change
    );

    let complete = store.apply_hook(AIHookEventPayload {
        kind: "turnCompleted".to_string(),
        updated_at: 1030.0,
        metadata: Some(AIHookEventMetadata {
            has_completed_turn: Some(true),
            ..empty_metadata()
        }),
        ..test_hook("turnCompleted", 1030.0)
    });
    assert!(complete.did_change);
    assert!(complete.completion.is_some());

    let probe = store.apply_runtime_snapshot(
        "terminal-1",
        AIRuntimeContextSnapshot {
            tool: "codex".to_string(),
            external_session_id: Some("session-1".to_string()),
            transcript_path: Some("/tmp/codex.jsonl".to_string()),
            model: Some("gpt-5.4".to_string()),
            assistant_preview: Some("done".to_string()),
            input_tokens: 0,
            output_tokens: 0,
            cached_input_tokens: 0,
            total_tokens: 200,
            updated_at: 1036.0,
            started_at: Some(1020.0),
            completed_at: Some(1030.0),
            response_state: Some("idle".to_string()),
            was_interrupted: false,
            has_completed_turn: true,
            session_origin: "live".to_string(),
            source: "probe".to_string(),
            plan: None,
        },
    );

    assert!(probe.did_change);
    assert!(probe.completion.is_none());
    let snapshot = store.snapshot();
    assert_eq!(snapshot.completion_count, 1);
    assert_eq!(snapshot.sessions[0].total_tokens, 200);
}

#[test]
fn same_session_next_prompt_completion_notifies_again() {
    let store = AIRuntimeStateStore::default();
    assert!(
        store
            .apply_hook(test_hook("sessionStarted", 1000.0))
            .did_change
    );
    assert!(
        store
            .apply_hook(test_hook("promptSubmitted", 1020.0))
            .did_change
    );
    assert!(
        store
            .apply_hook(AIHookEventPayload {
                kind: "turnCompleted".to_string(),
                updated_at: 1030.0,
                metadata: Some(AIHookEventMetadata {
                    has_completed_turn: Some(true),
                    ..empty_metadata()
                }),
                ..test_hook("turnCompleted", 1030.0)
            })
            .completion
            .is_some()
    );

    assert!(
        store
            .apply_hook(test_hook("promptSubmitted", 1040.0))
            .did_change
    );
    let second = store.apply_hook(AIHookEventPayload {
        kind: "turnCompleted".to_string(),
        updated_at: 1050.0,
        metadata: Some(AIHookEventMetadata {
            has_completed_turn: Some(true),
            ..empty_metadata()
        }),
        ..test_hook("turnCompleted", 1050.0)
    });

    assert!(second.did_change);
    assert!(second.completion.is_some());
}

fn test_hook(kind: &str, updated_at: f64) -> AIHookEventPayload {
    AIHookEventPayload {
        kind: kind.to_string(),
        terminal_id: "terminal-1".to_string(),
        terminal_instance_id: Some("instance-1".to_string()),
        project_id: "project-1".to_string(),
        project_name: "Project".to_string(),
        project_path: Some("/tmp/codex-project".to_string()),
        session_title: "Codex".to_string(),
        tool: "codex".to_string(),
        ai_session_id: Some("session-1".to_string()),
        model: Some("gpt-5.4".to_string()),
        input_tokens: None,
        output_tokens: None,
        cached_input_tokens: None,
        total_tokens: None,
        updated_at,
        metadata: None,
    }
}

fn test_hook_for(
    tool: &str,
    terminal_id: &str,
    ai_session_id: &str,
    updated_at: f64,
) -> AIHookEventPayload {
    AIHookEventPayload {
        tool: tool.to_string(),
        terminal_id: terminal_id.to_string(),
        terminal_instance_id: Some(format!("{terminal_id}-instance")),
        ai_session_id: Some(ai_session_id.to_string()),
        session_title: format!("{tool} session"),
        updated_at,
        ..test_hook("promptSubmitted", updated_at)
    }
}

fn empty_metadata() -> AIHookEventMetadata {
    AIHookEventMetadata {
        transcript_path: None,
        notification_type: None,
        source: None,
        reason: None,
        cwd: None,
        target_tool_name: None,
        message: None,
        was_interrupted: None,
        has_completed_turn: None,
    }
}
