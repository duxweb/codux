//! Desktop-only tests for the live-stats view (`stats_view`), which merges the
//! crate's `AIHistorySummary` with the desktop's `AIRuntimeStateSummary`. The
//! session/summary DB tests live in the `codux-ai-sessions` crate.

use super::*;
use crate::ai_history_normalized::{AIHeatmapDay, AITimeBucket, AIUsageBreakdownItem};
use crate::ai_runtime_state::{AIRuntimeSessionSummary, AIRuntimeStateSummary};

#[test]
fn stats_view_owns_display_token_mode_and_project_filtering() {
    let today_start = crate::ai_history_normalized::local_day_start_seconds(2_000.0);
    let history = AIHistorySummary {
        project_total_tokens: 100,
        project_cached_input_tokens: 40,
        today_total_tokens: 30,
        today_cached_input_tokens: 10,
        today_time_buckets: vec![AITimeBucket {
            start: today_start,
            end: today_start + 1800.0,
            input_tokens: 20,
            output_tokens: 10,
            total_tokens: 30,
            cached_input_tokens: 10,
            request_count: 2,
        }],
        heatmap: vec![AIHeatmapDay {
            day: today_start,
            input_tokens: 20,
            output_tokens: 10,
            total_tokens: 30,
            cached_input_tokens: 10,
            request_count: 2,
        }],
        tool_breakdown: vec![AIUsageBreakdownItem {
            key: "codex".to_string(),
            total_tokens: 100,
            cached_input_tokens: 40,
            request_count: 2,
            usage_amounts: Vec::new(),
        }],
        model_breakdown: vec![AIUsageBreakdownItem {
            key: "gpt-5".to_string(),
            total_tokens: 80,
            cached_input_tokens: 20,
            request_count: 1,
            usage_amounts: Vec::new(),
        }],
        ..Default::default()
    };
    let runtime = AIRuntimeStateSummary {
        sessions: vec![
            AIRuntimeSessionSummary {
                project_id: "project-a".to_string(),
                tool: "codex".to_string(),
                model: Some("gpt-5".to_string()),
                total_tokens: 5,
                cached_input_tokens: 3,
                ..runtime_session("term-a")
            },
            AIRuntimeSessionSummary {
                project_id: "project-b".to_string(),
                tool: "claude".to_string(),
                total_tokens: 50,
                cached_input_tokens: 50,
                ..runtime_session("term-b")
            },
        ],
        ..Default::default()
    };

    let normalized = stats_view(&history, &runtime, Some("project-a"), "normalized", 2_000.0);
    assert_eq!(normalized.project_total_tokens, 100);
    assert_eq!(normalized.today_total_tokens, 30);
    assert_eq!(normalized.today_buckets[0].value, 30);
    let normalized_heatmap_day = normalized
        .heatmap
        .iter()
        .find(|cell| cell.is_known)
        .expect("known heatmap day");
    assert_eq!(normalized_heatmap_day.value, 30);
    assert_eq!(normalized_heatmap_day.input_tokens, 20);
    assert_eq!(normalized_heatmap_day.output_tokens, 10);
    assert_eq!(normalized_heatmap_day.total_tokens, 30);
    assert_eq!(normalized_heatmap_day.cached_input_tokens, 10);
    assert_eq!(normalized.current_sessions.len(), 1);
    assert_eq!(normalized.current_sessions[0].session_id, "term-a");
    assert_eq!(
        normalized.current_sessions[0].terminal_id.as_deref(),
        Some("term-a")
    );
    assert_eq!(normalized.current_sessions[0].title, "Session");
    assert_eq!(normalized.current_sessions[0].status, "running");
    assert!(normalized.current_sessions[0].is_running);
    assert_eq!(normalized.current_sessions[0].total_tokens, 5);
    assert_eq!(normalized.tool_rows[0].value, 100);

    let with_cache = stats_view(
        &history,
        &runtime,
        Some("project-a"),
        "includingCache",
        2_000.0,
    );
    assert_eq!(with_cache.project_total_tokens, 140);
    assert_eq!(with_cache.today_total_tokens, 40);
    assert_eq!(with_cache.today_buckets[0].value, 40);
    let with_cache_heatmap_day = with_cache
        .heatmap
        .iter()
        .find(|cell| cell.is_known)
        .expect("known heatmap day");
    assert_eq!(with_cache_heatmap_day.value, 40);
    assert_eq!(with_cache_heatmap_day.input_tokens, 20);
    assert_eq!(with_cache_heatmap_day.output_tokens, 10);
    assert_eq!(with_cache_heatmap_day.total_tokens, 30);
    assert_eq!(with_cache_heatmap_day.cached_input_tokens, 10);
    assert_eq!(with_cache.current_sessions[0].total_tokens, 8);
    assert_eq!(with_cache.tool_rows[0].value, 140);
    assert_eq!(with_cache.model_rows[0].value, 100);
}

#[test]
fn stats_view_filters_current_sessions_by_selected_worktree_scope() {
    let runtime = AIRuntimeStateSummary {
        sessions: vec![
            AIRuntimeSessionSummary {
                project_id: "worktree-a".to_string(),
                tool: "codex".to_string(),
                total_tokens: 10,
                ..runtime_session("term-a")
            },
            AIRuntimeSessionSummary {
                project_id: "worktree-b".to_string(),
                tool: "codewhale".to_string(),
                total_tokens: 20,
                ..runtime_session("term-b")
            },
        ],
        ..Default::default()
    };

    let stats = stats_view(
        &AIHistorySummary::default(),
        &runtime,
        Some("worktree-b"),
        "normalized",
        2_000.0,
    );

    assert_eq!(stats.current_sessions.len(), 1);
    assert_eq!(stats.current_sessions[0].tool, "codewhale");
    assert_eq!(stats.current_sessions[0].total_tokens, 20);
}

fn runtime_session(terminal_id: &str) -> AIRuntimeSessionSummary {
    AIRuntimeSessionSummary {
        terminal_id: terminal_id.to_string(),
        project_id: String::new(),
        project_path: None,
        tool: String::new(),
        ai_session_id: None,
        model: None,
        state: "running".to_string(),
        project_name: "Project".to_string(),
        session_title: "Session".to_string(),
        started_at: None,
        updated_at: 2_000.0,
        event_count: 1,
        has_completed_turn: false,
        was_interrupted: false,
        notification_type: None,
        target_tool_name: None,
        message: None,
        latest_assistant_preview: None,
        total_tokens: 0,
        cached_input_tokens: 0,
        raw_total_tokens: 0,
        raw_cached_input_tokens: 0,
        baseline_total_tokens: 0,
        baseline_cached_input_tokens: 0,
        usage_amounts: Vec::new(),
        raw_usage_amounts: Vec::new(),
        baseline_usage_amounts: Vec::new(),
        source: "test".to_string(),
        plan: None,
    }
}
