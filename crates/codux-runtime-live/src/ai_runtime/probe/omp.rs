use crate::ai_runtime::{
    probe::paths::select_omp_session_path,
    snapshot::{AIRuntimeContextSnapshot, AIRuntimeProbeRequest, AIUsageAmountSnapshot},
    state::normalized_string,
};
use codux_ai_history::omp_session::parse_omp_session;

pub(crate) fn probe_omp_runtime(
    request: &AIRuntimeProbeRequest,
) -> Option<AIRuntimeContextSnapshot> {
    let project_path = normalized_string(request.project_path.as_deref())?;
    let external_session_id = normalized_string(request.external_session_id.as_deref());
    let path = select_omp_session_path(
        &project_path,
        external_session_id.as_deref(),
        request.transcript_path.as_deref(),
        request.started_at,
        &request.occupied_external_session_ids,
    )?;
    let session = parse_omp_session(&path)?;
    let session_id = session.id.clone().or(external_session_id.clone());
    let session_origin = omp_session_origin(&session, request.started_at);
    let usage_amounts = (session.usage.cost_usd > 0.0)
        .then(|| AIUsageAmountSnapshot {
            unit: "USD".to_string(),
            value: session.usage.cost_usd,
        })
        .into_iter()
        .collect();

    Some(AIRuntimeContextSnapshot {
        tool: "omp".to_string(),
        external_session_id: session_id,
        transcript_path: Some(path.display().to_string()),
        model: session.model,
        assistant_preview: None,
        input_tokens: session.usage.input_tokens,
        output_tokens: session.usage.output_tokens,
        cached_input_tokens: session.usage.cached_input_tokens(),
        total_tokens: session.usage.total_tokens(),
        usage_amounts,
        baseline_usage_amounts: Vec::new(),
        updated_at: session.updated_at.max(request.updated_at),
        started_at: session.created_at,
        completed_at: None,
        response_state: None,
        was_interrupted: false,
        has_completed_turn: false,
        session_origin,
        source: "probe".to_string(),
        plan: None,
    })
}

fn omp_session_origin(
    session: &codux_ai_history::omp_session::OmpSession,
    started_at: Option<f64>,
) -> String {
    if session.parent_session.is_some()
        || started_at.is_some_and(|started_at| {
            session
                .created_at
                .is_some_and(|created_at| created_at + 1.0 < started_at)
        })
    {
        "restored".to_string()
    } else {
        "fresh".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashSet, fs};

    #[test]
    fn transcript_usage_never_infers_runtime_state() {
        let dir = std::env::temp_dir().join(format!("codux-omp-probe-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.jsonl");
        write_session(&path, "session-1", "/tmp/project");
        let request = AIRuntimeProbeRequest {
            terminal_id: "terminal-1".to_string(),
            terminal_instance_id: None,
            project_id: "project-1".to_string(),
            project_path: Some("/tmp/project".to_string()),
            tool: "omp".to_string(),
            external_session_id: Some(path.display().to_string()),
            transcript_path: None,
            started_at: Some(1_900_000_000.0),
            updated_at: 1_784_441_000.0,
            occupied_external_session_ids: HashSet::new(),
        };

        let snapshot = probe_omp_runtime(&request).unwrap();

        assert_eq!(snapshot.external_session_id.as_deref(), Some("session-1"));
        assert_eq!(snapshot.model.as_deref(), Some("claude-sonnet-4-5"));
        assert_eq!(snapshot.input_tokens, 3);
        assert_eq!(snapshot.output_tokens, 191);
        assert_eq!(snapshot.cached_input_tokens, 1_689);
        assert_eq!(snapshot.total_tokens, 194);
        assert_eq!(snapshot.usage_amounts[0].unit, "USD");
        assert!((snapshot.usage_amounts[0].value - 0.009189).abs() < 0.000_000_1);
        assert_eq!(snapshot.response_state, None);
        assert!(!snapshot.has_completed_turn);
        assert_eq!(snapshot.session_origin, "restored");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn forked_session_uses_existing_usage_as_baseline() {
        let dir = std::env::temp_dir().join(format!("codux-omp-probe-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.jsonl");
        fs::write(
            &path,
            "{\"type\":\"session\",\"version\":3,\"id\":\"fork-1\",\"parentSession\":\"source-1\",\"timestamp\":\"2026-07-19T01:00:00Z\",\"cwd\":\"/tmp/project\"}\n",
        )
        .unwrap();
        let session = parse_omp_session(&path).unwrap();

        assert_eq!(
            omp_session_origin(&session, Some(1_784_400_000.0)),
            "restored"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    fn write_session(path: &std::path::Path, id: &str, cwd: &str) {
        fs::write(
            path,
            format!(
                "{{\"type\":\"session\",\"version\":3,\"id\":\"{id}\",\"timestamp\":\"2026-07-19T01:00:00Z\",\"cwd\":\"{cwd}\"}}\n\
                 {{\"type\":\"message\",\"timestamp\":\"2026-07-19T01:00:02Z\",\"message\":{{\"role\":\"assistant\",\"provider\":\"anthropic\",\"model\":\"claude-sonnet-4-5\",\"usage\":{{\"input\":3,\"output\":191,\"cacheRead\":5,\"cacheWrite\":1684,\"cost\":{{\"total\":0.009189}}}}}}}}\n"
            ),
        )
        .unwrap();
    }
}
