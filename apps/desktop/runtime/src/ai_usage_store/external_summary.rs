fn migrate_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        DROP TABLE IF EXISTS ai_history_file_usage_bucket;
        DROP TABLE IF EXISTS ai_history_file_session_link;
        DROP TABLE IF EXISTS ai_history_file_time_bucket;
        DROP TABLE IF EXISTS ai_history_file_day_usage;
        DROP TABLE IF EXISTS ai_history_file_session;
        DROP TABLE IF EXISTS ai_history_file_checkpoint;
        DROP TABLE IF EXISTS ai_history_file_state;
        DROP TABLE IF EXISTS ai_history_project_index_state;
        "#,
    )?;
    for statement in SCHEMA_STATEMENTS {
        if statement.contains("ai_history_meta") {
            continue;
        }
        conn.execute_batch(statement)?;
    }
    conn.execute(
        r#"
        INSERT INTO ai_history_meta (key, value)
        VALUES ('normalized_history_schema_version', ?1)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value;
        "#,
        params![NORMALIZED_HISTORY_SCHEMA_VERSION],
    )?;
    Ok(())
}

fn external_file_summary_from_parsed(
    source: &str,
    file_path: String,
    file_modified_at: f64,
    file_size: i64,
    project: &AIHistoryProjectRequest,
    parsed: ParsedHistory,
) -> AIExternalFileSummary {
    let mut sessions = HashMap::<String, ParsedSessionAccumulator>::new();
    let active_duration_by_session = active_duration_by_session_id(&parsed.events);

    for event in &parsed.events {
        let session =
            sessions
                .entry(event.session_id.clone())
                .or_insert_with(|| ParsedSessionAccumulator {
                    session_key: event.session_id.clone(),
                    first_seen_at: event.timestamp,
                    last_seen_at: event.timestamp,
                    ..Default::default()
                });
        session.first_seen_at = min_nonzero(session.first_seen_at, event.timestamp);
        session.last_seen_at = session.last_seen_at.max(event.timestamp);
        session.active_duration_seconds = session.active_duration_seconds.max(
            *active_duration_by_session
                .get(&event.session_id)
                .unwrap_or(&0),
        );
    }

    for entry in &parsed.entries {
        let session =
            sessions
                .entry(entry.session_id.clone())
                .or_insert_with(|| ParsedSessionAccumulator {
                    session_key: entry.session_id.clone(),
                    first_seen_at: entry.timestamp,
                    last_seen_at: entry.timestamp,
                    ..Default::default()
                });
        session.external_session_id = entry
            .external_session_id
            .clone()
            .or(session.external_session_id.clone());
        session.title = entry.session_title.clone().or(session.title.clone());
        session.last_model = entry.model.clone().or(session.last_model.clone());
        session.first_seen_at = min_nonzero(session.first_seen_at, entry.timestamp);
        session.last_seen_at = session.last_seen_at.max(entry.timestamp);
        session.active_duration_seconds = session.active_duration_seconds.max(
            *active_duration_by_session
                .get(&entry.session_id)
                .unwrap_or(&0),
        );
    }

    let mut buckets = HashMap::<(String, String, i64), AIUsageBucket>::new();
    for entry in &parsed.entries {
        let model = entry.model.clone().unwrap_or_else(|| "unknown".to_string());
        let bucket_start = half_hour_bucket_start(entry.timestamp);
        let session = sessions
            .entry(entry.session_id.clone())
            .or_insert_with(|| parsed_session_from_entry(entry));
        let bucket = buckets
            .entry((entry.session_id.clone(), model.clone(), bucket_start as i64))
            .or_insert_with(|| {
                usage_bucket_from_session(source, session, project, &model, bucket_start)
            });
        bucket.input_tokens += entry.input_tokens;
        bucket.output_tokens += entry.output_tokens;
        bucket.total_tokens += entry.total_tokens();
        bucket.cached_input_tokens += entry.cached_input_tokens;
    }

    for event in &parsed.events {
        if event.role != HistoryRole::User {
            continue;
        }
        let bucket_start = half_hour_bucket_start(event.timestamp);
        let session = sessions
            .entry(event.session_id.clone())
            .or_insert_with(|| parsed_session_from_event(event));
        let model = session
            .last_model
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        let bucket = buckets
            .entry((event.session_id.clone(), model.clone(), bucket_start as i64))
            .or_insert_with(|| {
                usage_bucket_from_session(source, session, project, &model, bucket_start)
            });
        bucket.request_count += 1;
    }

    let mut usage_buckets = buckets.into_values().collect::<Vec<_>>();
    usage_buckets.sort_by(|left, right| {
        left.bucket_start
            .total_cmp(&right.bucket_start)
            .then_with(|| left.source.cmp(&right.source))
            .then_with(|| left.session_key.cmp(&right.session_key))
            .then_with(|| left.model.cmp(&right.model))
    });

    AIExternalFileSummary {
        source: source.to_string(),
        file_path,
        file_modified_at,
        file_size,
        project_path: project.path.clone(),
        usage_buckets,
    }
}
