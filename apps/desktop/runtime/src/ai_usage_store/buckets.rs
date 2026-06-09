fn parsed_session_from_entry(entry: &HistoryEntry) -> ParsedSessionAccumulator {
    ParsedSessionAccumulator {
        session_key: entry.session_id.clone(),
        external_session_id: entry.external_session_id.clone(),
        title: entry.session_title.clone(),
        first_seen_at: entry.timestamp,
        last_seen_at: entry.timestamp,
        last_model: entry.model.clone(),
        active_duration_seconds: 0,
    }
}

fn parsed_session_from_event(event: &HistoryEvent) -> ParsedSessionAccumulator {
    ParsedSessionAccumulator {
        session_key: event.session_id.clone(),
        first_seen_at: event.timestamp,
        last_seen_at: event.timestamp,
        ..Default::default()
    }
}

fn usage_bucket_from_session(
    source: &str,
    session: &ParsedSessionAccumulator,
    project: &AIHistoryProjectRequest,
    model: &str,
    bucket_start: f64,
) -> AIUsageBucket {
    AIUsageBucket {
        source: source.to_string(),
        session_key: session.session_key.clone(),
        external_session_id: session.external_session_id.clone(),
        session_title: session
            .title
            .clone()
            .unwrap_or_else(|| project.name.clone()),
        model: Some(model.to_string()),
        project_id: project.id.clone(),
        project_name: project.name.clone(),
        bucket_start,
        bucket_end: bucket_start + 30.0 * 60.0,
        input_tokens: 0,
        output_tokens: 0,
        total_tokens: 0,
        cached_input_tokens: 0,
        request_count: 0,
        active_duration_seconds: session.active_duration_seconds,
        first_seen_at: session.first_seen_at,
        last_seen_at: session.last_seen_at,
    }
}

fn build_session_links(usage_buckets: &[AIUsageBucket]) -> Vec<NormalizedSessionLinkRow> {
    let mut map = HashMap::<String, NormalizedSessionLinkRow>::new();
    for bucket in usage_buckets {
        map.entry(bucket.session_key.clone())
            .and_modify(|session| {
                session.external_session_id = session
                    .external_session_id
                    .clone()
                    .or(bucket.external_session_id.clone());
                session.session_title =
                    preferred_string(Some(&session.session_title), Some(&bucket.session_title))
                        .unwrap_or_else(|| bucket.project_name.clone());
                session.first_seen_at = min_nonzero(session.first_seen_at, bucket.first_seen_at);
                session.last_seen_at = session.last_seen_at.max(bucket.last_seen_at);
                session.last_model = bucket.model.clone().or(session.last_model.clone());
                session.active_duration_seconds = session
                    .active_duration_seconds
                    .max(bucket.active_duration_seconds);
            })
            .or_insert_with(|| NormalizedSessionLinkRow {
                source: bucket.source.clone(),
                session_key: bucket.session_key.clone(),
                external_session_id: bucket.external_session_id.clone(),
                project_id: bucket.project_id.clone(),
                project_name: bucket.project_name.clone(),
                session_title: preferred_string(Some(&bucket.session_title), None)
                    .unwrap_or_else(|| bucket.project_name.clone()),
                first_seen_at: bucket.first_seen_at,
                last_seen_at: bucket.last_seen_at,
                last_model: bucket.model.clone(),
                active_duration_seconds: bucket.active_duration_seconds,
            });
    }
    let mut values = map.into_values().collect::<Vec<_>>();
    values.sort_by(|left, right| {
        right
            .last_seen_at
            .total_cmp(&left.last_seen_at)
            .then_with(|| left.session_key.cmp(&right.session_key))
    });
    values
}
