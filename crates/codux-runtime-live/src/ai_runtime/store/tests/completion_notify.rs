use super::super::*;
use super::fixtures::*;

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
            usage_amounts: Vec::new(),
            baseline_usage_amounts: Vec::new(),
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
            usage_amounts: Vec::new(),
            baseline_usage_amounts: Vec::new(),
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
#[test]
fn running_session_suppresses_project_completion_badge() {
    let store = AIRuntimeStateStore::default();
    assert!(
        store
            .apply_hook(test_hook_for("codex", "terminal-a", "session-a", 1000.0))
            .did_change
    );
    assert!(
        store
            .apply_hook(test_hook_for("codex", "terminal-b", "session-b", 1001.0))
            .did_change
    );
    let complete = store.apply_hook(AIHookEventPayload {
        kind: "turnCompleted".to_string(),
        updated_at: 1010.0,
        metadata: Some(AIHookEventMetadata {
            has_completed_turn: Some(true),
            ..empty_metadata()
        }),
        ..test_hook_for("codex", "terminal-a", "session-a", 1010.0)
    });

    assert!(complete.did_change);
    assert!(complete.completion.is_none());
    let snapshot = store.snapshot();
    assert_eq!(snapshot.running_count, 1);
    assert_eq!(snapshot.completion_count, 0);
    assert!(snapshot.latest_completion.is_none());
    assert!(matches!(
        snapshot.projects[0].project_phase,
        AIProjectPhase::Running { .. }
    ));
    assert!(matches!(
        snapshot.projects[0].completed_phase,
        AIProjectPhase::Idle
    ));
}
#[test]
fn dismissed_completion_does_not_reappear_while_another_session_runs() {
    let store = AIRuntimeStateStore::default();
    assert!(
        store
            .apply_hook(test_hook_for("codex", "terminal-a", "session-a", 1000.0))
            .did_change
    );
    let complete = store.apply_hook(AIHookEventPayload {
        kind: "turnCompleted".to_string(),
        updated_at: 1010.0,
        metadata: Some(AIHookEventMetadata {
            has_completed_turn: Some(true),
            ..empty_metadata()
        }),
        ..test_hook_for("codex", "terminal-a", "session-a", 1010.0)
    });
    assert!(complete.completion.is_some());
    assert!(store.dismiss_completion("project-1"));
    assert!(
        store
            .apply_hook(test_hook_for("codex", "terminal-b", "session-b", 1020.0))
            .did_change
    );

    let snapshot = store.snapshot();
    assert_eq!(snapshot.running_count, 1);
    assert_eq!(snapshot.completion_count, 0);
    assert!(snapshot.latest_completion.is_none());
    assert!(matches!(
        snapshot.projects[0].completed_phase,
        AIProjectPhase::Idle
    ));
}
#[test]
fn detected_idle_session_does_not_suppress_sibling_completion() {
    let store = AIRuntimeStateStore::default();
    let idle_terminal = AIRuntimeTerminalState {
        terminal_id: "terminal-b".to_string(),
        terminal_instance_id: Some("terminal-b-instance".to_string()),
        project_id: "project-1".to_string(),
        slot_id: "slot-b".to_string(),
        title: "Claude".to_string(),
        cwd: "/tmp/codex-project".to_string(),
        tool: None,
        is_active: false,
        session_key: None,
    };
    let detected =
        std::collections::HashMap::from([("terminal-b".to_string(), "claude".to_string())]);
    assert!(
        store
            .ensure_detected_sessions(std::slice::from_ref(&idle_terminal), &detected, 1000.0)
            .did_change
    );
    assert!(
        store
            .ensure_detected_sessions(&[idle_terminal], &detected, 1015.0)
            .did_change
    );
    assert!(
        store
            .apply_hook(test_hook_for("codex", "terminal-a", "session-a", 1020.0))
            .did_change
    );
    let complete = store.apply_hook(AIHookEventPayload {
        kind: "turnCompleted".to_string(),
        updated_at: 1030.0,
        metadata: Some(AIHookEventMetadata {
            has_completed_turn: Some(true),
            ..empty_metadata()
        }),
        ..test_hook_for("codex", "terminal-a", "session-a", 1030.0)
    });

    assert!(complete.did_change);
    assert!(complete.completion.is_some());
    let snapshot = store.snapshot();
    assert_eq!(snapshot.running_count, 0);
    assert_eq!(snapshot.completion_count, 1);
    assert!(snapshot.latest_completion.is_some());
    assert!(matches!(
        snapshot.projects[0].completed_phase,
        AIProjectPhase::Completed { .. }
    ));
}
#[test]
fn timed_out_unfinished_session_still_suppresses_old_completion() {
    let store = AIRuntimeStateStore::default();
    assert!(
        store
            .apply_hook(test_hook_for("codex", "terminal-a", "session-a", 1000.0))
            .did_change
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
                ..test_hook_for("codex", "terminal-a", "session-a", 1010.0)
            })
            .completion
            .is_some()
    );
    assert!(
        store
            .apply_hook(test_hook_for("codex", "terminal-b", "session-b", 1020.0))
            .did_change
    );
    assert!(
        store
            .reconcile_bridge_snapshot(&[AIRuntimeTerminalState {
                terminal_id: "terminal-b".to_string(),
                terminal_instance_id: Some("terminal-b-instance".to_string()),
                project_id: "project-1".to_string(),
                slot_id: "slot-b".to_string(),
                title: "Codex".to_string(),
                cwd: "/tmp/codex-project".to_string(),
                tool: None,
                is_active: false,
                session_key: None,
            }])
            .did_change
    );

    let snapshot = store.snapshot();
    assert_eq!(snapshot.running_count, 0);
    assert_eq!(snapshot.completion_count, 0);
    assert!(snapshot.latest_completion.is_none());
    assert!(matches!(
        snapshot.projects[0].completed_phase,
        AIProjectPhase::Idle
    ));
}
