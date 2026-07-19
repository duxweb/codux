fn parse_omp_history_file(
    project: &AIHistoryProjectRequest,
    file_path: &Path,
) -> ParsedHistory {
    let Some(session) = parse_omp_session(file_path) else {
        return ParsedHistory::default();
    };
    if !paths_equivalent(session.cwd.as_deref(), &project.path) {
        return ParsedHistory::default();
    }
    let session_id = session
        .id
        .clone()
        .unwrap_or_else(|| deterministic_uuid(&file_path.display().to_string()));
    let session_title = session.title.clone().or_else(|| Some(project.name.clone()));
    let mut result = ParsedHistory::default();
    for event in session.events {
        result.events.push(HistoryEvent {
            source: "omp".to_string(),
            session_id: session_id.clone(),
            timestamp: event.timestamp,
            kind: match event.role {
                OmpSessionRole::User => HistoryEventKind::Request,
                OmpSessionRole::Assistant => HistoryEventKind::Activity,
            },
        });
        let Some(usage) = event.usage else {
            continue;
        };
        let usage_amounts = (usage.cost_usd > 0.0)
            .then(|| AIUsageAmount {
                unit: "USD".to_string(),
                value: usage.cost_usd,
            })
            .into_iter()
            .collect();
        result.entries.push(HistoryEntry {
            source: "omp".to_string(),
            session_id: session_id.clone(),
            external_session_id: session.id.clone(),
            session_title: session_title.clone(),
            timestamp: event.timestamp,
            model: event.model.or_else(|| session.model.clone()),
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cached_input_tokens: usage.cached_input_tokens(),
            reasoning_output_tokens: 0,
            usage_amounts,
        });
    }
    result.sessions.push(HistorySessionMetadata {
        source: "omp".to_string(),
        session_id,
        external_session_id: session.id,
        session_title,
        timestamp: session
            .created_at
            .unwrap_or_else(|| (session.updated_at > 0.0).then_some(session.updated_at).unwrap_or_else(now_seconds)),
        model: session.model,
    });
    result
}
