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
fn reconcile_without_live_terminal_marks_running_session_interrupted() {
    let store = AIRuntimeStateStore::default();
    assert!(
        store
            .apply_hook(test_hook("promptSubmitted", 1000.0))
            .did_change
    );

    let mutation = store.reconcile_bridge_snapshot(&[]);

    assert!(mutation.did_change);
    assert!(mutation.completion.is_some());
    let snapshot = store.snapshot();
    assert_eq!(snapshot.running_count, 0);
    assert_eq!(snapshot.completion_count, 1);
    assert_eq!(snapshot.sessions[0].state, "idle");
    assert!(snapshot.sessions[0].was_interrupted);
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
