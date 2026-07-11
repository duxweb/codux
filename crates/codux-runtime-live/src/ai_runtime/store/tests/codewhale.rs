use super::super::*;
use super::fixtures::*;

#[test]
fn codewhale_hook_is_tracked_as_runtime_session() {
    let store = AIRuntimeStateStore::default();
    let mutation = store.apply_hook(test_hook_for(
        "codewhale",
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
fn detected_codewhale_terminal_is_canonicalized_not_filtered() {
    let terminal = AIRuntimeTerminalState {
        terminal_id: "codewhale-term-1".to_string(),
        project_id: "project-1".to_string(),
        slot_id: "slot-1".to_string(),
        title: "CodeWhale".to_string(),
        cwd: "/tmp/codewhale-project".to_string(),
        tool: None,
        is_active: false,
        session_key: None,
        terminal_instance_id: Some("instance-1".to_string()),
    };

    let session = detected_terminal_session(&terminal, "codewhale", 1000.0).expect("session");

    assert_eq!(session.tool, "codewhale");
    assert_eq!(session.terminal_id, "codewhale-term-1");
    assert_eq!(session.project_name, "codewhale-project");
    assert_eq!(session.state, "idle");
    assert!(session.ai_session_id.is_none());
}
#[test]
fn screen_signal_does_not_start_codewhale_detected_turn() {
    let store = AIRuntimeStateStore::default();
    let terminal = AIRuntimeTerminalState {
        terminal_id: "codewhale-term-1".to_string(),
        project_id: "project-1".to_string(),
        slot_id: "slot-1".to_string(),
        title: "CodeWhale".to_string(),
        cwd: "/tmp/codewhale-project".to_string(),
        tool: None,
        is_active: false,
        session_key: None,
        terminal_instance_id: Some("instance-1".to_string()),
    };
    let detected = std::collections::HashMap::from([(
        "codewhale-term-1".to_string(),
        "codewhale".to_string(),
    )]);
    assert!(
        store
            .ensure_detected_sessions(&[terminal], &detected, 1000.0)
            .did_change
    );
    assert_eq!(store.snapshot().sessions[0].state, "idle");

    let mutation = store.apply_screen_signal("codewhale-term-1", ScreenSignal::Running);

    assert!(!mutation.did_change);
    let snapshot = store.snapshot();
    assert_eq!(snapshot.sessions[0].tool, "codewhale");
    assert_eq!(snapshot.sessions[0].state, "idle");
    assert_eq!(snapshot.running_count, 0);
}
#[test]
fn process_liveness_retires_undetected_hookless_sessions() {
    let store = AIRuntimeStateStore::default();
    let terminal = AIRuntimeTerminalState {
        terminal_id: "kiro-term-1".to_string(),
        project_id: "project-1".to_string(),
        slot_id: "slot-1".to_string(),
        title: "Kiro".to_string(),
        cwd: "/tmp/kiro-project".to_string(),
        tool: None,
        is_active: false,
        session_key: None,
        terminal_instance_id: Some("instance-1".to_string()),
    };
    assert!(
        store
            .apply_hook(test_hook_for(
                "kiro",
                "kiro-term-1",
                "kiro-session-1",
                1000.0
            ))
            .did_change
    );
    assert_eq!(store.snapshot().sessions[0].state, "responding");

    let shell_pids = vec![("kiro-term-1".to_string(), 1234)];
    let empty_detected = std::collections::HashMap::new();
    let first = store.retire_undetected_hookless_sessions(
        std::slice::from_ref(&terminal),
        &shell_pids,
        &empty_detected,
        1007.0,
    );

    assert!(first.did_change);
    let snapshot = store.snapshot();
    assert_eq!(snapshot.sessions[0].state, "idle");
    assert!(!snapshot.sessions[0].has_completed_turn);
    assert!(!snapshot.sessions[0].was_interrupted);

    let second = store.retire_undetected_hookless_sessions(
        &[terminal],
        &shell_pids,
        &empty_detected,
        1008.0,
    );

    assert!(second.did_change);
    assert!(store.snapshot().sessions.is_empty());
}
#[test]
fn process_liveness_does_not_retire_hook_driven_tools() {
    let store = AIRuntimeStateStore::default();
    let terminal = AIRuntimeTerminalState {
        terminal_id: "codex-term-1".to_string(),
        project_id: "project-1".to_string(),
        slot_id: "slot-1".to_string(),
        title: "Codex".to_string(),
        cwd: "/tmp/codex-project".to_string(),
        tool: None,
        is_active: false,
        session_key: None,
        terminal_instance_id: Some("instance-1".to_string()),
    };
    assert!(
        store
            .apply_hook(test_hook_for("codex", "codex-term-1", "session-1", 1000.0))
            .did_change
    );

    let mutation = store.retire_undetected_hookless_sessions(
        &[terminal],
        &[("codex-term-1".to_string(), 1234)],
        &std::collections::HashMap::new(),
        1010.0,
    );

    assert!(!mutation.did_change);
    assert_eq!(store.snapshot().sessions[0].state, "responding");
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

    let mut complete = test_hook_for("codewhale", "codewhale-term-1", "session-1", 1010.0);
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
fn codewhale_interrupted_turn_end_clears_loading() {
    let store = AIRuntimeStateStore::default();
    let prompt = test_hook_for("codewhale", "codewhale-term-1", "session-1", 1000.0);
    assert!(store.apply_hook(prompt).did_change);
    assert_eq!(store.snapshot().sessions[0].state, "responding");

    let mut interrupted = test_hook_for("codewhale", "codewhale-term-1", "session-1", 1010.0);
    interrupted.kind = "turnCompleted".to_string();
    interrupted.metadata = Some(AIHookEventMetadata {
        was_interrupted: Some(true),
        has_completed_turn: Some(false),
        reason: Some("interrupted".to_string()),
        ..empty_metadata()
    });
    assert!(store.apply_hook(interrupted).did_change);

    let snapshot = store.snapshot();
    let session = &snapshot.sessions[0];
    assert_eq!(snapshot.running_count, 0);
    assert_eq!(session.state, "idle");
    assert!(!session.is_running);
    assert!(session.was_interrupted);
    assert!(!session.has_completed_turn);
}
#[test]
fn codewhale_interrupted_turn_end_is_authoritative_over_responding_probe() {
    let previous = AISessionSnapshot {
        tool: "codewhale".to_string(),
        terminal_id: "codewhale-term-1".to_string(),
        terminal_instance_id: Some("instance-1".to_string()),
        project_id: "project-1".to_string(),
        project_name: "Project".to_string(),
        project_path: Some("/tmp/codewhale-project".to_string()),
        session_title: "CodeWhale".to_string(),
        ai_session_id: Some("session-1".to_string()),
        model: Some("deepseek-v4-flash".to_string()),
        state: "responding".to_string(),
        status: "running".to_string(),
        is_running: true,
        input_tokens: 0,
        output_tokens: 0,
        cached_input_tokens: 0,
        total_tokens: 51_562,
        baseline_total_tokens: 51_562,
        baseline_cached_input_tokens: 0,
        usage_amounts: Vec::new(),
        baseline_usage_amounts: Vec::new(),
        baseline_resolved: true,
        started_at: Some(1000.0),
        updated_at: 1000.0,
        active_turn_started_at: Some(1000.0),
        runtime_turn_started_at: None,
        completed_turn_started_at: None,
        session_origin: None,
        has_completed_turn: false,
        was_interrupted: false,
        transcript_path: Some("/tmp/codewhale-project/session-1.json".to_string()),
        notification_type: None,
        target_tool_name: None,
        message: None,
        latest_assistant_preview: None,
        plan: None,
    };
    let resolved = merge_snapshot_into_hook(
        AIHookEventPayload {
            kind: "turnCompleted".to_string(),
            tool: "codewhale".to_string(),
            terminal_id: "codewhale-term-1".to_string(),
            terminal_instance_id: Some("instance-1".to_string()),
            project_id: "project-1".to_string(),
            project_name: "Project".to_string(),
            project_path: Some("/tmp/codewhale-project".to_string()),
            session_title: "CodeWhale".to_string(),
            ai_session_id: Some("session-1".to_string()),
            model: Some("deepseek-v4-flash".to_string()),
            input_tokens: None,
            output_tokens: None,
            cached_input_tokens: None,
            total_tokens: Some(51_562),
            updated_at: 1010.0,
            metadata: Some(AIHookEventMetadata {
                was_interrupted: Some(true),
                has_completed_turn: Some(false),
                reason: Some("interrupted".to_string()),
                source: Some("codewhale-lifecycle".to_string()),
                ..empty_metadata()
            }),
        },
        AIRuntimeContextSnapshot {
            tool: "codewhale".to_string(),
            external_session_id: Some("session-1".to_string()),
            transcript_path: Some("/tmp/codewhale-project/session-1.json".to_string()),
            model: Some("deepseek-v4-flash".to_string()),
            assistant_preview: None,
            input_tokens: 0,
            output_tokens: 0,
            cached_input_tokens: 0,
            total_tokens: 51_562,
            usage_amounts: Vec::new(),
            baseline_usage_amounts: Vec::new(),
            updated_at: 1011.0,
            started_at: Some(990.0),
            completed_at: None,
            response_state: Some("responding".to_string()),
            was_interrupted: false,
            has_completed_turn: false,
            session_origin: "fresh".to_string(),
            source: "probe".to_string(),
            plan: None,
        },
        Some(&previous),
    );

    assert_eq!(resolved.kind, "turnCompleted");
    assert_eq!(
        resolved
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.was_interrupted),
        Some(true)
    );
    assert_eq!(
        resolved
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.has_completed_turn),
        Some(false)
    );
}
#[test]
fn codewhale_lifecycle_turn_end_is_authoritative_over_responding_probe() {
    let previous = AISessionSnapshot {
        tool: "codewhale".to_string(),
        terminal_id: "codewhale-term-1".to_string(),
        terminal_instance_id: Some("instance-1".to_string()),
        project_id: "project-1".to_string(),
        project_name: "Project".to_string(),
        project_path: Some("/tmp/codewhale-project".to_string()),
        session_title: "CodeWhale".to_string(),
        ai_session_id: Some("session-1".to_string()),
        model: Some("deepseek-v4-flash".to_string()),
        state: "responding".to_string(),
        status: "running".to_string(),
        is_running: true,
        input_tokens: 0,
        output_tokens: 0,
        cached_input_tokens: 0,
        total_tokens: 51_562,
        baseline_total_tokens: 51_562,
        baseline_cached_input_tokens: 0,
        usage_amounts: Vec::new(),
        baseline_usage_amounts: Vec::new(),
        baseline_resolved: true,
        started_at: Some(1000.0),
        updated_at: 1000.0,
        active_turn_started_at: Some(1000.0),
        runtime_turn_started_at: None,
        completed_turn_started_at: None,
        session_origin: None,
        has_completed_turn: false,
        was_interrupted: false,
        transcript_path: Some("/tmp/codewhale-project/session-1.json".to_string()),
        notification_type: None,
        target_tool_name: None,
        message: None,
        latest_assistant_preview: None,
        plan: None,
    };
    let resolved = merge_snapshot_into_hook(
        AIHookEventPayload {
            kind: "turnCompleted".to_string(),
            tool: "codewhale".to_string(),
            terminal_id: "codewhale-term-1".to_string(),
            terminal_instance_id: Some("instance-1".to_string()),
            project_id: "project-1".to_string(),
            project_name: "Project".to_string(),
            project_path: Some("/tmp/codewhale-project".to_string()),
            session_title: "CodeWhale".to_string(),
            ai_session_id: Some("session-1".to_string()),
            model: Some("deepseek-v4-flash".to_string()),
            input_tokens: None,
            output_tokens: None,
            cached_input_tokens: None,
            total_tokens: Some(51_562),
            updated_at: 1010.0,
            metadata: Some(AIHookEventMetadata {
                was_interrupted: Some(false),
                has_completed_turn: Some(true),
                reason: Some("unknown".to_string()),
                source: Some("codewhale-lifecycle".to_string()),
                ..empty_metadata()
            }),
        },
        AIRuntimeContextSnapshot {
            tool: "codewhale".to_string(),
            external_session_id: Some("session-1".to_string()),
            transcript_path: Some("/tmp/codewhale-project/session-1.json".to_string()),
            model: Some("deepseek-v4-flash".to_string()),
            assistant_preview: None,
            input_tokens: 0,
            output_tokens: 0,
            cached_input_tokens: 0,
            total_tokens: 51_562,
            usage_amounts: Vec::new(),
            baseline_usage_amounts: Vec::new(),
            updated_at: 1011.0,
            started_at: Some(990.0),
            completed_at: None,
            response_state: Some("responding".to_string()),
            was_interrupted: false,
            has_completed_turn: false,
            session_origin: "fresh".to_string(),
            source: "probe".to_string(),
            plan: None,
        },
        Some(&previous),
    );

    assert_eq!(resolved.kind, "turnCompleted");
    assert_eq!(
        resolved
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.was_interrupted),
        Some(false)
    );
    assert_eq!(
        resolved
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.has_completed_turn),
        Some(true)
    );
}
#[test]
fn codewhale_interrupted_turn_end_clears_loading_when_session_file_still_looks_responding() {
    let root = std::env::temp_dir().join(format!(
        "codux-codewhale-interrupt-probe-{}",
        Uuid::new_v4()
    ));
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
                    "model": "deepseek-v4-flash",
                    "total_tokens": 51562,
                    "created_at": "2026-06-29T05:00:00Z",
                    "updated_at": "2026-06-29T05:04:40Z"
                }},
                "messages": [
                    {{ "role": "user", "content": "interrupt me" }}
                ]
            }}"#,
            project.display()
        ),
    )
    .unwrap();

    let store = AIRuntimeStateStore::default();
    let mut prompt = test_hook_for(
        "codewhale",
        "codewhale-term-1",
        "session-1",
        1_782_630_000.0,
    );
    prompt.project_path = Some(project.display().to_string());
    prompt.total_tokens = Some(51_562);
    assert!(store.apply_hook(prompt).did_change);

    let mut interrupted = test_hook_for(
        "codewhale",
        "codewhale-term-1",
        "session-1",
        1_782_630_010.0,
    );
    interrupted.kind = "turnCompleted".to_string();
    interrupted.project_path = Some(project.display().to_string());
    interrupted.total_tokens = Some(51_562);
    interrupted.metadata = Some(AIHookEventMetadata {
        transcript_path: Some(session_file.display().to_string()),
        was_interrupted: Some(true),
        has_completed_turn: Some(false),
        reason: Some("interrupted".to_string()),
        source: Some("codewhale-lifecycle".to_string()),
        ..empty_metadata()
    });
    assert!(store.apply_hook(interrupted).did_change);

    let snapshot = store.snapshot();
    let session = &snapshot.sessions[0];
    assert_eq!(snapshot.running_count, 0);
    assert_eq!(session.state, "idle");
    assert!(!session.is_running);
    assert!(session.was_interrupted);
    assert!(!session.has_completed_turn);

    let stale_probe = store.apply_runtime_snapshot(
        "codewhale-term-1",
        AIRuntimeContextSnapshot {
            tool: "codewhale".to_string(),
            external_session_id: Some("session-1".to_string()),
            transcript_path: Some(session_file.display().to_string()),
            model: Some("deepseek-v4-flash".to_string()),
            assistant_preview: None,
            input_tokens: 0,
            output_tokens: 0,
            cached_input_tokens: 0,
            total_tokens: 51_562,
            usage_amounts: Vec::new(),
            baseline_usage_amounts: Vec::new(),
            updated_at: 1_782_630_020.0,
            started_at: Some(1_782_630_000.0),
            completed_at: None,
            response_state: Some("responding".to_string()),
            was_interrupted: false,
            has_completed_turn: false,
            session_origin: "fresh".to_string(),
            source: "probe".to_string(),
            plan: None,
        },
    );
    assert!(!stale_probe.did_change);
    let snapshot = store.snapshot();
    assert_eq!(snapshot.running_count, 0);
    assert_eq!(snapshot.sessions[0].state, "idle");
    assert!(snapshot.sessions[0].was_interrupted);

    let _ = fs::remove_dir_all(root);
}
#[test]
fn codewhale_prompt_with_existing_total_starts_current_usage_at_zero() {
    let mut core = AIRuntimeStateCore::default();
    let prompt = AIHookEventPayload {
        total_tokens: Some(51_562),
        ..test_hook_for("codewhale", "codewhale-term-1", "session-1", 1000.0)
    };

    assert!(apply_hook_unlocked(&mut core, prompt));

    let session = core.sessions.get("codewhale-term-1").unwrap();
    assert_eq!(session.total_tokens, 51_562);
    assert_eq!(session.baseline_total_tokens, 51_562);
    assert!(session.baseline_resolved);
    assert_eq!(
        summary::project_totals_unlocked(&core, Some("project-1"), now_seconds()).total_tokens,
        0
    );
}
#[test]
fn codewhale_restored_prompt_keeps_existing_total_as_baseline() {
    let previous = AISessionSnapshot {
        tool: "codewhale".to_string(),
        terminal_id: "codewhale-term-1".to_string(),
        terminal_instance_id: Some("instance-1".to_string()),
        project_id: "project-1".to_string(),
        project_name: "Project".to_string(),
        project_path: Some("/tmp/codewhale-project".to_string()),
        session_title: "CodeWhale".to_string(),
        ai_session_id: Some("session-1".to_string()),
        model: Some("deepseek-v4-flash".to_string()),
        state: "idle".to_string(),
        status: "idle".to_string(),
        is_running: false,
        input_tokens: 0,
        output_tokens: 0,
        cached_input_tokens: 0,
        total_tokens: 51_562,
        baseline_total_tokens: 51_562,
        baseline_cached_input_tokens: 0,
        usage_amounts: Vec::new(),
        baseline_usage_amounts: Vec::new(),
        baseline_resolved: true,
        started_at: Some(1000.0),
        updated_at: 1000.0,
        active_turn_started_at: None,
        runtime_turn_started_at: None,
        completed_turn_started_at: None,
        session_origin: None,
        has_completed_turn: true,
        was_interrupted: false,
        transcript_path: Some("/tmp/codewhale-project/session-1.json".to_string()),
        notification_type: None,
        target_tool_name: None,
        message: None,
        latest_assistant_preview: None,
        plan: None,
    };

    let resolved = merge_snapshot_into_hook(
        AIHookEventPayload {
            kind: "promptSubmitted".to_string(),
            tool: "codewhale".to_string(),
            terminal_id: "codewhale-term-1".to_string(),
            terminal_instance_id: Some("instance-1".to_string()),
            project_id: "project-1".to_string(),
            project_name: "Project".to_string(),
            project_path: Some("/tmp/codewhale-project".to_string()),
            session_title: "CodeWhale".to_string(),
            ai_session_id: Some("session-1".to_string()),
            model: Some("deepseek-v4-flash".to_string()),
            input_tokens: None,
            output_tokens: None,
            cached_input_tokens: None,
            total_tokens: None,
            updated_at: 1010.0,
            metadata: None,
        },
        AIRuntimeContextSnapshot {
            tool: "codewhale".to_string(),
            external_session_id: Some("session-1".to_string()),
            transcript_path: Some("/tmp/codewhale-project/session-1.json".to_string()),
            model: Some("deepseek-v4-flash".to_string()),
            assistant_preview: Some("done".to_string()),
            input_tokens: 0,
            output_tokens: 0,
            cached_input_tokens: 0,
            total_tokens: 132_786,
            usage_amounts: Vec::new(),
            baseline_usage_amounts: Vec::new(),
            updated_at: 1020.0,
            started_at: Some(900.0),
            completed_at: Some(1020.0),
            response_state: Some("idle".to_string()),
            was_interrupted: false,
            has_completed_turn: true,
            session_origin: "restored".to_string(),
            source: "probe".to_string(),
            plan: None,
        },
        Some(&previous),
    );

    assert_eq!(resolved.kind, "promptSubmitted");
    assert_eq!(resolved.total_tokens, Some(51_562));
}
#[test]
fn codewhale_restored_session_prompt_resets_current_usage_baseline() {
    let mut core = AIRuntimeStateCore::default();
    assert!(apply_hook_unlocked(
        &mut core,
        AIHookEventPayload {
            total_tokens: Some(276_000),
            ..test_hook_for("codewhale", "codewhale-term-1", "session-1", 1000.0)
        }
    ));
    assert!(apply_hook_unlocked(
        &mut core,
        AIHookEventPayload {
            kind: "turnCompleted".to_string(),
            total_tokens: Some(276_000),
            metadata: Some(AIHookEventMetadata {
                has_completed_turn: Some(true),
                ..empty_metadata()
            }),
            ..test_hook_for("codewhale", "codewhale-term-1", "session-1", 1010.0)
        }
    ));
    let session = core.sessions.get("codewhale-term-1").unwrap();
    assert_eq!(session.baseline_total_tokens, 276_000);
    assert_eq!(
        summary::project_totals_unlocked(&core, Some("project-1"), now_seconds()).total_tokens,
        0
    );

    assert!(apply_hook_unlocked(
        &mut core,
        AIHookEventPayload {
            kind: "promptSubmitted".to_string(),
            total_tokens: Some(276_000),
            ..test_hook_for("codewhale", "codewhale-term-1", "session-1", 2000.0)
        }
    ));

    let session = core.sessions.get("codewhale-term-1").unwrap();
    assert_eq!(session.state, "responding");
    assert_eq!(session.total_tokens, 276_000);
    assert_eq!(session.baseline_total_tokens, 276_000);
    assert_eq!(
        summary::project_totals_unlocked(&core, Some("project-1"), now_seconds()).total_tokens,
        0
    );

    assert!(apply_hook_unlocked(
        &mut core,
        AIHookEventPayload {
            kind: "turnCompleted".to_string(),
            total_tokens: Some(303_783),
            metadata: Some(AIHookEventMetadata {
                has_completed_turn: Some(true),
                ..empty_metadata()
            }),
            ..test_hook_for("codewhale", "codewhale-term-1", "session-1", 2010.0)
        }
    ));

    let session = core.sessions.get("codewhale-term-1").unwrap();
    assert_eq!(session.total_tokens, 303_783);
    assert_eq!(session.baseline_total_tokens, 276_000);
    assert_eq!(
        summary::project_totals_unlocked(&core, Some("project-1"), now_seconds()).total_tokens,
        27_783
    );
}
