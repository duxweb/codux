mod parse;
mod preview;
mod types;

use crate::ai_runtime::{
    probe::paths::find_codex_rollout_path,
    snapshot::{AIRuntimeContextSnapshot, AIRuntimeProbeRequest},
    state::normalized_string,
};

use self::parse::parse_codex_runtime_state;

pub(crate) fn probe_codex_runtime(
    request: &AIRuntimeProbeRequest,
) -> Option<AIRuntimeContextSnapshot> {
    let project_path = normalized_string(request.project_path.as_deref())?;
    let file_path = normalized_string(request.transcript_path.as_deref())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            let external_id = normalized_string(request.external_session_id.as_deref())?;
            find_codex_rollout_path(&project_path, &external_id)
        })?;
    let transcript_path = file_path.display().to_string();
    let parsed = parse_codex_runtime_state(
        &file_path,
        Some(&project_path),
        request.started_at,
        request.updated_at,
    )?;
    let external_session_id = normalized_string(request.external_session_id.as_deref());
    let mut plan = parsed.plan;
    if let (Some(plan), Some(session_id)) = (plan.as_mut(), external_session_id.as_ref()) {
        plan.session_id = session_id.clone();
    }
    Some(AIRuntimeContextSnapshot {
        tool: "codex".to_string(),
        external_session_id,
        transcript_path: Some(transcript_path),
        model: parsed.model,
        assistant_preview: parsed.assistant_preview,
        input_tokens: parsed.input_tokens.unwrap_or(0),
        output_tokens: parsed.output_tokens.unwrap_or(0),
        cached_input_tokens: parsed.cached_input_tokens.unwrap_or(0),
        total_tokens: parsed.total_tokens.unwrap_or(0),
        updated_at: parsed.updated_at.unwrap_or(request.updated_at),
        started_at: parsed.started_at,
        completed_at: parsed.completed_at,
        response_state: parsed.response_state,
        was_interrupted: parsed.was_interrupted,
        has_completed_turn: parsed.has_completed_turn,
        session_origin: parsed.origin,
        source: "probe".to_string(),
        plan,
    })
}
