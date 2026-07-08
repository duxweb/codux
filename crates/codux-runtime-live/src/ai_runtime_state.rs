use crate::ai_runtime::{
    AIPlanSnapshot, AIProjectPhase, AIProjectTotals, AIRuntimeStateSnapshot, AISessionSnapshot,
    AIUsageAmountSnapshot,
};
use codux_protocol::{RemoteAICurrentSession, RemoteAIUsageAmount};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::Path;

const STATE_SOURCE: &str = "memory";

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeStateSummary {
    pub path: String,
    pub updated_at: f64,
    pub running_count: usize,
    pub needs_input_count: usize,
    pub completed_count: usize,
    pub session_count: usize,
    pub global_total_tokens: i64,
    pub global_cached_input_tokens: i64,
    pub project_states: Vec<AIRuntimeProjectStateSummary>,
    pub project_totals: Vec<AIRuntimeProjectTotalsSummary>,
    pub sessions: Vec<AIRuntimeSessionSummary>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeProjectStateSummary {
    pub project_id: String,
    pub project_phase: AIRuntimeProjectPhaseSummary,
    pub completed_phase: AIRuntimeProjectPhaseSummary,
    pub totals: AIRuntimeProjectTotalsSummary,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeProjectPhaseSummary {
    pub kind: String,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub was_interrupted: bool,
    #[serde(default)]
    pub updated_at: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeProjectTotalsSummary {
    pub project_id: String,
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub running: usize,
    pub needs_input: usize,
    pub completed: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeSessionSummary {
    pub terminal_id: String,
    #[serde(default)]
    pub project_id: String,
    #[serde(default)]
    pub project_path: Option<String>,
    pub tool: String,
    #[serde(default, rename = "aiSessionId")]
    pub ai_session_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    pub state: String,
    pub project_name: String,
    pub session_title: String,
    #[serde(default)]
    pub started_at: Option<f64>,
    pub updated_at: f64,
    pub event_count: usize,
    #[serde(default)]
    pub has_completed_turn: bool,
    #[serde(default)]
    pub was_interrupted: bool,
    #[serde(default)]
    pub notification_type: Option<String>,
    #[serde(default)]
    pub target_tool_name: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub latest_assistant_preview: Option<String>,
    #[serde(default)]
    pub plan: Option<AIPlanSnapshot>,
    #[serde(default)]
    pub total_tokens: i64,
    #[serde(default)]
    pub cached_input_tokens: i64,
    #[serde(default)]
    pub raw_total_tokens: i64,
    #[serde(default)]
    pub raw_cached_input_tokens: i64,
    #[serde(default)]
    pub baseline_total_tokens: i64,
    #[serde(default)]
    pub baseline_cached_input_tokens: i64,
    #[serde(default)]
    pub usage_amounts: Vec<AIUsageAmountSnapshot>,
    #[serde(default)]
    pub raw_usage_amounts: Vec<AIUsageAmountSnapshot>,
    #[serde(default)]
    pub baseline_usage_amounts: Vec<AIUsageAmountSnapshot>,
    pub source: String,
}

pub struct AIRuntimeStateService;

impl AIRuntimeStateService {
    pub fn new(_support_dir: &Path) -> Self {
        Self
    }

    pub fn summary(&self) -> AIRuntimeStateSummary {
        summary_from_raw(STATE_SOURCE.to_string(), &Map::new(), None)
    }

    pub fn summary_from_runtime_snapshot(
        &self,
        snapshot: &AIRuntimeStateSnapshot,
    ) -> AIRuntimeStateSummary {
        // Map the typed runtime snapshot straight to the summary. The old path
        // serialized everything into a serde_json::Map and immediately parsed
        // it back into these same typed structs (plus a redundant second sort);
        // that round trip ran on every state rebuild (1Hz floor + the 500ms pet
        // loop). Direct mapping skips all of it.
        let mut sessions = snapshot
            .sessions
            .iter()
            .map(session_from_runtime_snapshot)
            .collect::<Vec<_>>();
        sessions.sort_by(|left, right| right.updated_at.total_cmp(&left.updated_at));
        let project_states = snapshot
            .projects
            .iter()
            .map(|project| AIRuntimeProjectStateSummary {
                project_id: project.project_id.clone(),
                project_phase: (&project.project_phase).into(),
                completed_phase: (&project.completed_phase).into(),
                totals: project_totals_summary(&project.project_id, &project.totals),
            })
            .collect::<Vec<_>>();
        let project_totals = snapshot
            .projects
            .iter()
            .map(|project| project_totals_summary(&project.project_id, &project.totals))
            .collect::<Vec<_>>();
        AIRuntimeStateSummary {
            path: STATE_SOURCE.to_string(),
            updated_at: snapshot.updated_at,
            running_count: snapshot.running_count,
            needs_input_count: snapshot.needs_input_count,
            completed_count: snapshot.completion_count,
            session_count: sessions.len(),
            global_total_tokens: snapshot.global_totals.total_tokens,
            global_cached_input_tokens: snapshot.global_totals.cached_input_tokens,
            project_states,
            project_totals,
            sessions,
            error: None,
        }
    }
}

fn project_totals_summary(
    project_id: &str,
    totals: &AIProjectTotals,
) -> AIRuntimeProjectTotalsSummary {
    AIRuntimeProjectTotalsSummary {
        project_id: project_id.to_string(),
        total_tokens: totals.total_tokens,
        cached_input_tokens: totals.cached_input_tokens,
        running: totals.running,
        needs_input: totals.needs_input,
        completed: totals.completed,
    }
}

pub fn remote_current_sessions_from_runtime_state(
    runtime_state: &AIRuntimeStateSummary,
    project_id: &str,
) -> Vec<RemoteAICurrentSession> {
    runtime_state
        .sessions
        .iter()
        .filter(|session| project_id.trim().is_empty() || session.project_id == project_id)
        .map(remote_current_session_from_runtime_session)
        .collect()
}

fn remote_current_session_from_runtime_session(
    session: &AIRuntimeSessionSummary,
) -> RemoteAICurrentSession {
    RemoteAICurrentSession {
        session_id: session
            .ai_session_id
            .clone()
            .unwrap_or_else(|| session.terminal_id.clone()),
        terminal_id: Some(session.terminal_id.clone()),
        project_id: session.project_id.clone(),
        title: session.session_title.clone(),
        tool: session.tool.clone(),
        model: session.model.clone(),
        status: session.state.clone(),
        is_running: session.state == "running",
        total_tokens: session.raw_total_tokens.max(session.total_tokens).max(0),
        cached_input_tokens: session
            .raw_cached_input_tokens
            .max(session.cached_input_tokens)
            .max(0),
        current_total_tokens: session.total_tokens.max(0),
        current_cached_input_tokens: session.cached_input_tokens.max(0),
        usage_amounts: remote_usage_amounts(&session.raw_usage_amounts),
        current_usage_amounts: remote_usage_amounts(&session.usage_amounts),
    }
}

fn remote_usage_amounts(amounts: &[AIUsageAmountSnapshot]) -> Vec<RemoteAIUsageAmount> {
    amounts
        .iter()
        .filter(|amount| !amount.unit.trim().is_empty() && amount.value > 0.0)
        .map(|amount| RemoteAIUsageAmount {
            unit: amount.unit.clone(),
            value: amount.value,
        })
        .collect()
}

fn summary_from_raw(
    path: String,
    raw: &Map<String, Value>,
    error: Option<String>,
) -> AIRuntimeStateSummary {
    let mut sessions = raw_sessions(raw);
    sessions.sort_by(|left, right| right.updated_at.total_cmp(&left.updated_at));
    let running_count = raw
        .get("runningCount")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or_else(|| {
            sessions
                .iter()
                .filter(|session| session.state == "running")
                .count()
        });
    let needs_input_count = raw
        .get("needsInputCount")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or_else(|| {
            sessions
                .iter()
                .filter(|session| session.state == "needs-input")
                .count()
        });
    let completed_count = raw
        .get("completedCount")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or_else(|| {
            sessions
                .iter()
                .filter(|session| session.state == "completed")
                .count()
        });
    AIRuntimeStateSummary {
        path,
        updated_at: raw.get("updatedAt").and_then(Value::as_f64).unwrap_or(0.0),
        running_count,
        needs_input_count,
        completed_count,
        session_count: raw
            .get("sessionCount")
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(sessions.len()),
        global_total_tokens: raw_global_totals(raw).total_tokens,
        global_cached_input_tokens: raw_global_totals(raw).cached_input_tokens,
        project_states: raw_project_states(raw),
        project_totals: raw_project_totals(raw),
        sessions,
        error,
    }
}

fn raw_global_totals(raw: &Map<String, Value>) -> AIRuntimeProjectTotalsSummary {
    raw.get("globalTotals")
        .map(|value| runtime_totals_from_value(String::new(), value))
        .unwrap_or_default()
}

fn raw_project_states(raw: &Map<String, Value>) -> Vec<AIRuntimeProjectStateSummary> {
    raw.get("projects")
        .and_then(Value::as_array)
        .map(|projects| {
            projects
                .iter()
                .filter_map(|project| {
                    let project_id = project
                        .get("projectId")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    (!project_id.is_empty()).then(|| AIRuntimeProjectStateSummary {
                        totals: runtime_totals_from_value(
                            project_id.clone(),
                            project.get("totals").unwrap_or(&Value::Null),
                        ),
                        project_id,
                        project_phase: runtime_phase_from_value(project.get("projectPhase")),
                        completed_phase: runtime_phase_from_value(project.get("completedPhase")),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn runtime_phase_from_value(value: Option<&Value>) -> AIRuntimeProjectPhaseSummary {
    let Some(value) = value else {
        return AIRuntimeProjectPhaseSummary::default();
    };
    let Some(kind) = value.get("kind").and_then(Value::as_str) else {
        return AIRuntimeProjectPhaseSummary::default();
    };
    AIRuntimeProjectPhaseSummary {
        kind: kind.to_string(),
        tool: value
            .get("tool")
            .and_then(Value::as_str)
            .map(str::to_string),
        was_interrupted: value
            .get("wasInterrupted")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        updated_at: value
            .get("updatedAt")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
    }
}

fn raw_project_totals(raw: &Map<String, Value>) -> Vec<AIRuntimeProjectTotalsSummary> {
    raw.get("projects")
        .and_then(Value::as_array)
        .map(|projects| {
            projects
                .iter()
                .filter_map(|project| {
                    let project_id = project
                        .get("projectId")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    (!project_id.is_empty()).then(|| {
                        runtime_totals_from_value(
                            project_id,
                            project.get("totals").unwrap_or(&Value::Null),
                        )
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

impl From<&AIProjectPhase> for AIRuntimeProjectPhaseSummary {
    fn from(phase: &AIProjectPhase) -> Self {
        match phase {
            AIProjectPhase::Idle => Self {
                kind: "idle".to_string(),
                ..Self::default()
            },
            AIProjectPhase::Running { tool } => Self {
                kind: "running".to_string(),
                tool: Some(tool.clone()),
                ..Self::default()
            },
            AIProjectPhase::NeedsInput { tool } => Self {
                kind: "needsInput".to_string(),
                tool: Some(tool.clone()),
                ..Self::default()
            },
            AIProjectPhase::Completed {
                tool,
                was_interrupted,
                updated_at,
            } => Self {
                kind: "completed".to_string(),
                tool: Some(tool.clone()),
                was_interrupted: *was_interrupted,
                updated_at: *updated_at,
            },
        }
    }
}

fn runtime_totals_from_value(project_id: String, value: &Value) -> AIRuntimeProjectTotalsSummary {
    let total = value.as_object();
    AIRuntimeProjectTotalsSummary {
        project_id,
        total_tokens: total
            .and_then(|total| total.get("totalTokens"))
            .and_then(Value::as_i64)
            .unwrap_or(0),
        cached_input_tokens: total
            .and_then(|total| total.get("cachedInputTokens"))
            .and_then(Value::as_i64)
            .unwrap_or(0),
        running: total
            .and_then(|total| total.get("running"))
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(0),
        needs_input: total
            .and_then(|total| total.get("needsInput"))
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(0),
        completed: total
            .and_then(|total| total.get("completed"))
            .and_then(Value::as_u64)
            .map(|value| value as usize)
            .unwrap_or(0),
    }
}

fn raw_sessions(raw: &Map<String, Value>) -> Vec<AIRuntimeSessionSummary> {
    raw.get("sessions")
        .and_then(Value::as_array)
        .map(|sessions| {
            sessions
                .iter()
                .filter_map(|session| {
                    serde_json::from_value::<AIRuntimeSessionSummary>(session.clone()).ok()
                })
                .collect()
        })
        .unwrap_or_default()
}

fn session_from_runtime_snapshot(session: &AISessionSnapshot) -> AIRuntimeSessionSummary {
    AIRuntimeSessionSummary {
        terminal_id: session.terminal_id.clone(),
        project_id: session.project_id.clone(),
        project_path: session.project_path.clone(),
        tool: session.tool.clone(),
        ai_session_id: session.ai_session_id.clone(),
        model: session.model.clone(),
        state: runtime_snapshot_session_state(session).to_string(),
        project_name: session.project_name.clone(),
        session_title: session.session_title.clone(),
        started_at: session.started_at,
        updated_at: session.updated_at,
        event_count: usize::from(session.started_at.is_some())
            + usize::from(session.has_completed_turn)
            + usize::from(session.notification_type.is_some()),
        has_completed_turn: session.has_completed_turn,
        was_interrupted: session.was_interrupted,
        notification_type: session.notification_type.clone(),
        target_tool_name: session.target_tool_name.clone(),
        message: session.message.clone(),
        latest_assistant_preview: session.latest_assistant_preview.clone(),
        plan: session.plan.clone(),
        total_tokens: (session.total_tokens - session.baseline_total_tokens).max(0),
        cached_input_tokens: (session.cached_input_tokens - session.baseline_cached_input_tokens)
            .max(0),
        raw_total_tokens: session.total_tokens.max(0),
        raw_cached_input_tokens: session.cached_input_tokens.max(0),
        baseline_total_tokens: session.baseline_total_tokens.max(0),
        baseline_cached_input_tokens: session.baseline_cached_input_tokens.max(0),
        usage_amounts: subtract_usage_amounts(
            &session.usage_amounts,
            &session.baseline_usage_amounts,
        ),
        raw_usage_amounts: session.usage_amounts.clone(),
        baseline_usage_amounts: session.baseline_usage_amounts.clone(),
        source: "supervisor".to_string(),
    }
}

fn subtract_usage_amounts(
    totals: &[AIUsageAmountSnapshot],
    baselines: &[AIUsageAmountSnapshot],
) -> Vec<AIUsageAmountSnapshot> {
    totals
        .iter()
        .filter_map(|total| {
            let unit = total.unit.trim();
            if unit.is_empty() {
                return None;
            }
            let baseline = baselines
                .iter()
                .find(|item| item.unit == total.unit)
                .map(|item| item.value)
                .unwrap_or(0.0);
            let value = (total.value - baseline).max(0.0);
            (value > 0.0).then(|| AIUsageAmountSnapshot {
                unit: total.unit.clone(),
                value,
            })
        })
        .collect()
}

fn runtime_snapshot_session_state(session: &AISessionSnapshot) -> &'static str {
    if session.state == "needsInput" || session.notification_type.is_some() {
        "needs-input"
    } else if session.is_running || session.state == "responding" {
        "running"
    } else if session.has_completed_turn {
        "completed"
    } else {
        "idle"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::{AIPlanItem, AIPlanSnapshot, AIProjectTotals};

    #[test]
    fn summary_returns_default_memory_state() {
        let dir = std::env::temp_dir();
        let summary = AIRuntimeStateService::new(&dir).summary();

        assert_eq!(summary.session_count, 0);
        assert_eq!(summary.running_count, 0);
        assert_eq!(summary.path, STATE_SOURCE);
    }

    #[test]
    fn summary_from_runtime_snapshot_returns_live_supervisor_state() {
        let dir = std::env::temp_dir();
        let service = AIRuntimeStateService::new(&dir);
        let snapshot = AIRuntimeStateSnapshot {
            running_count: 1,
            needs_input_count: 1,
            completion_count: 0,
            global_totals: AIProjectTotals {
                total_tokens: 100,
                cached_input_tokens: 15,
                running: 1,
                needs_input: 1,
                completed: 0,
            },
            projects: vec![crate::ai_runtime::AIProjectStateSnapshot {
                project_id: "project-a".to_string(),
                project_phase: crate::ai_runtime::AIProjectPhase::Running {
                    tool: "codex".to_string(),
                },
                completed_phase: crate::ai_runtime::AIProjectPhase::Idle,
                totals: AIProjectTotals {
                    total_tokens: 100,
                    cached_input_tokens: 15,
                    running: 1,
                    needs_input: 0,
                    completed: 0,
                },
            }],
            updated_at: 42.0,
            sessions: vec![
                AISessionSnapshot {
                    terminal_id: "term-a".to_string(),
                    terminal_instance_id: None,
                    project_id: "project-a".to_string(),
                    project_name: "Codux".to_string(),
                    project_path: None,
                    session_title: "Build".to_string(),
                    tool: "codex".to_string(),
                    ai_session_id: Some("session-a".to_string()),
                    model: Some("gpt-5".to_string()),
                    state: "responding".to_string(),
                    status: "running".to_string(),
                    is_running: true,
                    input_tokens: 0,
                    output_tokens: 0,
                    cached_input_tokens: 20,
                    total_tokens: 150,
                    baseline_total_tokens: 50,
                    baseline_cached_input_tokens: 5,
                    usage_amounts: vec![AIUsageAmountSnapshot {
                        unit: "credit".to_string(),
                        value: 0.041,
                    }],
                    baseline_usage_amounts: vec![AIUsageAmountSnapshot {
                        unit: "credit".to_string(),
                        value: 0.011,
                    }],
                    baseline_resolved: false,
                    started_at: Some(10.0),
                    updated_at: 20.0,
                    active_turn_started_at: None,
                    runtime_turn_started_at: None,
                    completed_turn_started_at: None,
                    session_origin: None,
                    has_completed_turn: false,
                    was_interrupted: false,
                    transcript_path: None,
                    notification_type: None,
                    target_tool_name: None,
                    message: None,
                    latest_assistant_preview: None,
                    plan: Some(AIPlanSnapshot {
                        source: "codex".to_string(),
                        session_id: "session-a".to_string(),
                        updated_at: 20.0,
                        items: vec![AIPlanItem {
                            text: "Patch parser".to_string(),
                            status: "in_progress".to_string(),
                            priority: None,
                        }],
                    }),
                },
                AISessionSnapshot {
                    terminal_id: "term-b".to_string(),
                    terminal_instance_id: None,
                    project_id: "project-b".to_string(),
                    project_name: "Codux".to_string(),
                    project_path: None,
                    session_title: "Review".to_string(),
                    tool: "claude".to_string(),
                    ai_session_id: None,
                    model: None,
                    state: "needsInput".to_string(),
                    status: "needs input".to_string(),
                    is_running: false,
                    input_tokens: 0,
                    output_tokens: 0,
                    cached_input_tokens: 0,
                    total_tokens: 0,
                    baseline_total_tokens: 0,
                    baseline_cached_input_tokens: 0,
                    usage_amounts: Vec::new(),
                    baseline_usage_amounts: Vec::new(),
                    baseline_resolved: false,
                    started_at: Some(11.0),
                    updated_at: 30.0,
                    active_turn_started_at: None,
                    runtime_turn_started_at: None,
                    completed_turn_started_at: None,
                    session_origin: None,
                    has_completed_turn: false,
                    was_interrupted: false,
                    transcript_path: None,
                    notification_type: Some("approval".to_string()),
                    target_tool_name: None,
                    message: None,
                    latest_assistant_preview: None,
                    plan: None,
                },
            ],
            ..Default::default()
        };

        let summary = service.summary_from_runtime_snapshot(&snapshot);

        assert_eq!(summary.session_count, 2);
        assert_eq!(summary.running_count, 1);
        assert_eq!(summary.needs_input_count, 1);
        assert_eq!(summary.global_total_tokens, 100);
        assert_eq!(summary.global_cached_input_tokens, 15);
        assert_eq!(summary.project_states.len(), 1);
        assert_eq!(summary.project_states[0].project_id, "project-a");
        assert_eq!(summary.project_states[0].project_phase.kind, "running");
        assert_eq!(
            summary.project_states[0].project_phase.tool.as_deref(),
            Some("codex")
        );
        assert_eq!(summary.project_states[0].completed_phase.kind, "idle");
        assert_eq!(summary.project_totals.len(), 1);
        assert_eq!(summary.project_totals[0].project_id, "project-a");
        assert_eq!(summary.project_totals[0].total_tokens, 100);
        assert_eq!(summary.sessions[0].terminal_id, "term-b");
        assert_eq!(summary.sessions[0].state, "needs-input");
        assert_eq!(summary.sessions[1].state, "running");
        assert_eq!(summary.sessions[1].project_id, "project-a");
        assert_eq!(
            summary.sessions[1].ai_session_id.as_deref(),
            Some("session-a")
        );
        assert_eq!(summary.sessions[1].raw_total_tokens, 150);
        assert_eq!(summary.sessions[1].raw_cached_input_tokens, 20);
        assert_eq!(summary.sessions[1].baseline_total_tokens, 50);
        assert_eq!(summary.sessions[1].baseline_cached_input_tokens, 5);
        assert_eq!(summary.sessions[1].total_tokens, 100);
        assert_eq!(summary.sessions[1].cached_input_tokens, 15);
        assert_eq!(summary.sessions[1].model.as_deref(), Some("gpt-5"));
        assert_eq!(summary.sessions[1].total_tokens, 100);
        assert_eq!(summary.sessions[1].cached_input_tokens, 15);
        let remote_sessions = remote_current_sessions_from_runtime_state(&summary, "project-a");
        assert_eq!(remote_sessions.len(), 1);
        assert_eq!(remote_sessions[0].total_tokens, 150);
        assert_eq!(remote_sessions[0].cached_input_tokens, 20);
        assert_eq!(remote_sessions[0].current_total_tokens, 100);
        assert_eq!(remote_sessions[0].current_cached_input_tokens, 15);
        assert_eq!(remote_sessions[0].usage_amounts[0].unit, "credit");
        assert!((remote_sessions[0].usage_amounts[0].value - 0.041).abs() < f64::EPSILON);
        assert_eq!(remote_sessions[0].current_usage_amounts[0].unit, "credit");
        assert!((remote_sessions[0].current_usage_amounts[0].value - 0.03).abs() < 0.000_001);
        let plan = summary.sessions[1].plan.as_ref().expect("plan");
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].text, "Patch parser");
        assert_eq!(plan.items[0].status, "in_progress");

        assert_eq!(summary.path, STATE_SOURCE);
    }
}
