use super::super::*;
use super::fixtures::*;

#[test]
fn ensure_detected_sessions_creates_idle_session_without_hook_or_active_binding() {
    let store = AIRuntimeStateStore::default();
    // A plain terminal binding: no `is_active`, no `session_key` — exactly what
    // production upserts. Hook-free discovery must still create a session purely
    // from the process-detected tool.
    let terminal = crate::ai_runtime::registry::AIRuntimeTerminalState {
        terminal_id: "terminal-1".to_string(),
        terminal_instance_id: Some("instance-1".to_string()),
        project_id: "project-1".to_string(),
        slot_id: "slot-1".to_string(),
        title: "zsh".to_string(),
        cwd: "/tmp/codex-project".to_string(),
        tool: None,
        is_active: false,
        session_key: None,
    };
    let detected =
        std::collections::HashMap::from([("terminal-1".to_string(), "codex".to_string())]);

    let mutation = store.ensure_detected_sessions(&[terminal.clone()], &detected, 1000.0);
    assert!(mutation.did_change);

    let snapshot = store.snapshot();
    assert_eq!(snapshot.sessions.len(), 1);
    assert_eq!(snapshot.sessions[0].terminal_id, "terminal-1");
    assert_eq!(snapshot.sessions[0].tool, "codex");
    assert_eq!(snapshot.sessions[0].state, "idle");
    assert!(snapshot.sessions[0].ai_session_id.is_none());

    // Idempotent: a second detection on the same terminal does not duplicate or
    // clobber the existing session.
    let again = store.ensure_detected_sessions(&[terminal], &detected, 1001.0);
    assert!(again.did_change);
    assert_eq!(store.snapshot().sessions.len(), 1);
    assert_eq!(store.snapshot().sessions[0].updated_at, 1001.0);
}
#[test]
fn ensure_detected_sessions_switches_same_terminal_to_new_tool() {
    let store = AIRuntimeStateStore::default();
    let terminal = crate::ai_runtime::registry::AIRuntimeTerminalState {
        terminal_id: "terminal-1".to_string(),
        terminal_instance_id: Some("instance-1".to_string()),
        project_id: "project-1".to_string(),
        slot_id: "slot-1".to_string(),
        title: "zsh".to_string(),
        cwd: "/tmp/project".to_string(),
        tool: None,
        is_active: false,
        session_key: None,
    };
    assert!(
        store
            .ensure_detected_sessions(
                &[terminal.clone()],
                &std::collections::HashMap::from([(
                    "terminal-1".to_string(),
                    "opencode".to_string(),
                )]),
                1000.0,
            )
            .did_change
    );
    assert!(
        store
            .apply_runtime_snapshot(
                "terminal-1",
                AIRuntimeContextSnapshot {
                    tool: "opencode".to_string(),
                    external_session_id: Some("opencode-session".to_string()),
                    transcript_path: Some("/tmp/opencode.db".to_string()),
                    model: Some("gpt-5.4".to_string()),
                    assistant_preview: Some("done".to_string()),
                    input_tokens: 20_000,
                    output_tokens: 1_800,
                    cached_input_tokens: 0,
                    total_tokens: 21_800,
                    usage_amounts: Vec::new(),
                    baseline_usage_amounts: Vec::new(),
                    updated_at: 1010.0,
                    started_at: Some(1001.0),
                    completed_at: Some(1010.0),
                    response_state: Some("idle".to_string()),
                    was_interrupted: false,
                    has_completed_turn: true,
                    session_origin: "fresh".to_string(),
                    source: "probe".to_string(),
                    plan: None,
                },
            )
            .did_change
    );
    assert_eq!(store.snapshot().sessions[0].tool, "opencode");

    assert!(
        store
            .ensure_detected_sessions(
                &[terminal],
                &std::collections::HashMap::from([("terminal-1".to_string(), "agy".to_string(),)]),
                1020.0,
            )
            .did_change
    );

    let snapshot = store.snapshot();
    assert_eq!(snapshot.sessions.len(), 1);
    assert_eq!(snapshot.sessions[0].terminal_id, "terminal-1");
    assert_eq!(snapshot.sessions[0].tool, "agy");
    assert!(snapshot.sessions[0].ai_session_id.is_none());
    assert!(snapshot.sessions[0].model.is_none());
    assert_eq!(snapshot.sessions[0].total_tokens, 0);
    assert_eq!(snapshot.sessions[0].state, "idle");
}
#[test]
fn ensure_detected_sessions_refreshes_existing_idle_kiro_session() {
    let store = AIRuntimeStateStore::default();
    let terminal = crate::ai_runtime::registry::AIRuntimeTerminalState {
        terminal_id: "terminal-kiro".to_string(),
        terminal_instance_id: Some("instance-1".to_string()),
        project_id: "project-1".to_string(),
        slot_id: "slot-1".to_string(),
        title: "Kiro".to_string(),
        cwd: "/tmp/kiro-project".to_string(),
        tool: None,
        is_active: false,
        session_key: None,
    };
    let detected =
        std::collections::HashMap::from([("terminal-kiro".to_string(), "kiro".to_string())]);

    assert!(
        store
            .ensure_detected_sessions(&[terminal.clone()], &detected, 1000.0)
            .did_change
    );
    assert!(
        store
            .apply_runtime_snapshot(
                "terminal-kiro",
                AIRuntimeContextSnapshot {
                    tool: "kiro".to_string(),
                    external_session_id: Some("kiro-session-1".to_string()),
                    transcript_path: Some("/tmp/kiro-session-1.json".to_string()),
                    model: Some("auto".to_string()),
                    assistant_preview: Some("done".to_string()),
                    input_tokens: 0,
                    output_tokens: 0,
                    cached_input_tokens: 0,
                    total_tokens: 0,
                    usage_amounts: vec![crate::ai_runtime::AIUsageAmountSnapshot {
                        unit: "credit".to_string(),
                        value: 0.03,
                    }],
                    baseline_usage_amounts: Vec::new(),
                    updated_at: 1010.0,
                    started_at: Some(1005.0),
                    completed_at: Some(1010.0),
                    response_state: Some("idle".to_string()),
                    was_interrupted: false,
                    has_completed_turn: true,
                    session_origin: "live".to_string(),
                    source: "probe".to_string(),
                    plan: None,
                },
            )
            .did_change
    );

    assert!(
        !store
            .ensure_detected_sessions(&[terminal], &detected, 2000.0)
            .did_change
    );
    let snapshot = store.snapshot();
    assert_eq!(snapshot.sessions[0].tool, "kiro");
    assert_eq!(snapshot.sessions[0].state, "idle");
    assert_eq!(snapshot.sessions[0].updated_at, 1010.0);
}
#[test]
fn runtime_binding_creates_idle_session_without_process_detection() {
    let store = AIRuntimeStateStore::default();

    let mutation = store.apply_binding(test_binding("binding-1", "terminal-1", "instance-1", 10.0));

    assert!(mutation.did_change);
    let snapshot = store.snapshot();
    assert_eq!(snapshot.sessions.len(), 1);
    assert_eq!(snapshot.sessions[0].terminal_id, "terminal-1");
    assert_eq!(
        snapshot.sessions[0].terminal_instance_id.as_deref(),
        Some("instance-1")
    );
    assert_eq!(snapshot.sessions[0].tool, "codex");
    assert_eq!(snapshot.sessions[0].state, "idle");
    assert_eq!(snapshot.sessions[0].started_at, Some(10.0));
}
#[test]
fn runtime_binding_replaces_reused_terminal_with_new_instance() {
    let store = AIRuntimeStateStore::default();
    assert!(
        store
            .apply_binding(test_binding("binding-1", "terminal-1", "instance-1", 10.0))
            .did_change
    );
    assert!(
        store
            .apply_binding(test_binding("binding-2", "terminal-1", "instance-2", 20.0))
            .did_change
    );

    let snapshot = store.snapshot();
    assert_eq!(snapshot.sessions.len(), 1);
    assert_eq!(
        snapshot.sessions[0].terminal_instance_id.as_deref(),
        Some("instance-2")
    );
    assert_eq!(snapshot.sessions[0].started_at, Some(20.0));
}
#[test]
fn runtime_binding_resets_reused_terminal_for_new_ai_process() {
    let store = AIRuntimeStateStore::default();
    assert!(
        store
            .apply_binding(test_binding("binding-1", "terminal-1", "instance-1", 10.0))
            .did_change
    );
    assert!(
        store
            .apply_runtime_snapshot(
                "terminal-1",
                AIRuntimeContextSnapshot {
                    tool: "codex".to_string(),
                    external_session_id: Some("old-session".to_string()),
                    transcript_path: Some("/tmp/old.jsonl".to_string()),
                    model: Some("old-model".to_string()),
                    assistant_preview: Some("old preview".to_string()),
                    input_tokens: 400,
                    output_tokens: 100,
                    cached_input_tokens: 50,
                    total_tokens: 500,
                    usage_amounts: Vec::new(),
                    baseline_usage_amounts: Vec::new(),
                    updated_at: 12.0,
                    started_at: Some(11.0),
                    completed_at: None,
                    response_state: Some("responding".to_string()),
                    was_interrupted: false,
                    has_completed_turn: false,
                    session_origin: "fresh".to_string(),
                    source: "probe".to_string(),
                    plan: None,
                },
            )
            .did_change
    );

    let mut next_binding = test_binding("binding-2", "terminal-1", "instance-1", 20.0);
    next_binding.external_session_id = Some("new-session".to_string());
    assert!(store.apply_binding(next_binding).did_change);

    let snapshot = store.snapshot();
    assert_eq!(snapshot.sessions.len(), 1);
    let session = &snapshot.sessions[0];
    assert_eq!(session.terminal_id, "terminal-1");
    assert_eq!(session.terminal_instance_id.as_deref(), Some("instance-1"));
    assert_eq!(session.ai_session_id.as_deref(), Some("new-session"));
    assert!(session.transcript_path.is_none());
    assert!(session.model.is_none());
    assert_eq!(session.state, "idle");
    assert_eq!(session.total_tokens, 0);
    assert_eq!(session.baseline_total_tokens, 0);
    assert_eq!(session.started_at, Some(20.0));
}
#[test]
fn binding_first_old_probe_snapshot_becomes_baseline_not_current_usage() {
    let mut core = AIRuntimeStateCore::default();
    let binding = test_binding("binding-1", "terminal-1", "instance-1", 1000.0);
    core.sessions
        .insert("terminal-1".to_string(), binding_terminal_session(&binding));

    assert!(apply_runtime_snapshot_unlocked(
        &mut core,
        "terminal-1",
        AIRuntimeContextSnapshot {
            tool: "codex".to_string(),
            external_session_id: Some("old-session".to_string()),
            transcript_path: Some("/tmp/old-codex.jsonl".to_string()),
            model: Some("gpt-5.5".to_string()),
            assistant_preview: None,
            input_tokens: 4_000,
            output_tokens: 1_000,
            cached_input_tokens: 2_000,
            total_tokens: 5_000,
            usage_amounts: Vec::new(),
            baseline_usage_amounts: Vec::new(),
            updated_at: 1000.0,
            started_at: Some(900.0),
            completed_at: Some(950.0),
            response_state: Some("idle".to_string()),
            was_interrupted: false,
            has_completed_turn: true,
            session_origin: "unknown".to_string(),
            source: "probe".to_string(),
            plan: None,
        }
    ));

    let session = core.sessions.get("terminal-1").unwrap();
    assert_eq!(session.baseline_total_tokens, 5_000);
    assert_eq!(session.baseline_cached_input_tokens, 2_000);
    assert_eq!(session.total_tokens, 5_000);
    assert_eq!(
        summary::project_totals_unlocked(&core, Some("project-1"), now_seconds()).total_tokens,
        0
    );
}
#[test]
fn probe_backfills_logical_session_identity_on_existing_terminal_binding() {
    let mut core = AIRuntimeStateCore::default();
    let binding = test_binding("binding-1", "terminal-1", "instance-1", 1000.0);
    core.sessions
        .insert("terminal-1".to_string(), binding_terminal_session(&binding));

    assert!(apply_runtime_snapshot_unlocked(
        &mut core,
        "terminal-1",
        AIRuntimeContextSnapshot {
            tool: "kimi".to_string(),
            external_session_id: Some("driver-session-1".to_string()),
            transcript_path: Some("/tmp/kimi/wire.jsonl".to_string()),
            model: Some("kimi-k2".to_string()),
            assistant_preview: None,
            input_tokens: 1_000,
            output_tokens: 200,
            cached_input_tokens: 300,
            total_tokens: 1_200,
            usage_amounts: Vec::new(),
            baseline_usage_amounts: Vec::new(),
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

    assert_eq!(core.sessions.len(), 1);
    let session = core.sessions.get("terminal-1").unwrap();
    assert_eq!(session.tool, "kimi");
    assert_eq!(session.ai_session_id.as_deref(), Some("driver-session-1"));
    assert_eq!(
        session.transcript_path.as_deref(),
        Some("/tmp/kimi/wire.jsonl")
    );
    assert_eq!(session.model.as_deref(), Some("kimi-k2"));
    assert_eq!(session.state, "responding");
    assert_eq!(session.baseline_total_tokens, 1_200);
    assert_eq!(
        summary::project_totals_unlocked(&core, Some("project-1"), now_seconds()).total_tokens,
        0
    );
}

#[test]
fn restored_binding_first_probe_becomes_baseline_even_when_probe_origin_is_unknown() {
    let mut core = AIRuntimeStateCore::default();
    let mut binding = test_binding("binding-1", "terminal-1", "instance-1", 1000.0);
    binding.external_session_id = Some("restored-session".to_string());
    binding.session_origin = Some("restored".to_string());
    core.sessions
        .insert("terminal-1".to_string(), binding_terminal_session(&binding));

    assert!(apply_runtime_snapshot_unlocked(
        &mut core,
        "terminal-1",
        AIRuntimeContextSnapshot {
            tool: "codex".to_string(),
            external_session_id: Some("restored-session".to_string()),
            transcript_path: Some("/tmp/restored-codex.jsonl".to_string()),
            model: Some("gpt-5.5".to_string()),
            assistant_preview: None,
            input_tokens: 18_000_000,
            output_tokens: 5_400_000,
            cached_input_tokens: 0,
            total_tokens: 23_400_000,
            usage_amounts: Vec::new(),
            baseline_usage_amounts: Vec::new(),
            updated_at: 1001.0,
            started_at: Some(1001.0),
            completed_at: None,
            response_state: Some("responding".to_string()),
            was_interrupted: false,
            has_completed_turn: false,
            session_origin: "unknown".to_string(),
            source: "probe".to_string(),
            plan: None,
        }
    ));

    let session = core.sessions.get("terminal-1").unwrap();
    assert_eq!(session.total_tokens, 23_400_000);
    assert_eq!(session.baseline_total_tokens, 23_400_000);
    assert!(session.baseline_resolved);
    assert_eq!(session.session_origin, None);
    assert_eq!(
        summary::project_totals_unlocked(&core, Some("project-1"), now_seconds()).total_tokens,
        0
    );
}
