fn migrate_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch("BEGIN IMMEDIATE TRANSACTION;")?;
    let result = (|| -> Result<()> {
        ensure_usage_event_columns(conn)?;
        conn.execute_batch(
            r#"
        INSERT OR IGNORE INTO ai_history_file_usage_event (
            source, file_path, project_path, project_id, event_ordinal,
            session_key, occurred_at, total_tokens, request_count,
            active_duration_seconds
        )
        SELECT
            bucket.source,
            bucket.file_path,
            bucket.project_path,
            COALESCE(
                NULLIF(MAX(link.project_id), ''),
                NULLIF(MAX(project.project_id), ''),
                bucket.project_path
            ),
            bucket.rowid,
            bucket.session_key,
            CAST(bucket.bucket_start AS INTEGER),
            bucket.total_tokens,
            0,
            0
        FROM ai_history_file_usage_bucket AS bucket
        LEFT JOIN ai_history_file_session_link AS link
          ON link.source = bucket.source
         AND link.file_path = bucket.file_path
         AND link.project_path = bucket.project_path
         AND link.session_key = bucket.session_key
        LEFT JOIN ai_history_project_index_state AS project
          ON project.project_path = bucket.project_path
        WHERE NOT EXISTS (
            SELECT 1
            FROM ai_history_file_usage_event AS event
            WHERE event.source = bucket.source
              AND event.file_path = bucket.file_path
              AND event.project_path = bucket.project_path
        )
        GROUP BY bucket.rowid;

        INSERT OR IGNORE INTO ai_history_file_usage_event (
            source, file_path, project_path, project_id, event_ordinal,
            session_key, occurred_at, total_tokens, request_count,
            active_duration_seconds
        )
        SELECT
            bucket.source,
            bucket.file_path,
            bucket.project_path,
            COALESCE(
                NULLIF(MAX(link.project_id), ''),
                NULLIF(MAX(project.project_id), ''),
                bucket.project_path
            ),
            -bucket.rowid,
            bucket.session_key,
            CAST(bucket.bucket_start AS INTEGER),
            0,
            bucket.request_count,
            bucket.active_duration_seconds
        FROM ai_history_file_usage_bucket AS bucket
        LEFT JOIN ai_history_file_session_link AS link
          ON link.source = bucket.source
         AND link.file_path = bucket.file_path
         AND link.project_path = bucket.project_path
         AND link.session_key = bucket.session_key
        LEFT JOIN ai_history_project_index_state AS project
          ON project.project_path = bucket.project_path
        WHERE bucket.request_count > 0 OR bucket.active_duration_seconds > 0
        GROUP BY bucket.rowid;

        UPDATE ai_history_file_usage_event
        SET project_id = COALESCE(
            NULLIF((
                SELECT link.project_id
                FROM ai_history_file_session_link AS link
                WHERE link.source = ai_history_file_usage_event.source
                  AND link.file_path = ai_history_file_usage_event.file_path
                  AND link.project_path = ai_history_file_usage_event.project_path
                  AND link.session_key = ai_history_file_usage_event.session_key
                LIMIT 1
            ), ''),
            NULLIF((
                SELECT project.project_id
                FROM ai_history_project_index_state AS project
                WHERE project.project_path = ai_history_file_usage_event.project_path
                LIMIT 1
            ), ''),
            project_path
        )
        WHERE project_id = '';
        "#,
        )?;
        canonicalize_usage_event_paths(conn)?;
        conn.execute_batch(
            r#"
        DROP TABLE IF EXISTS ai_history_file_usage_bucket;
        DROP TABLE IF EXISTS ai_history_file_usage_amount;
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
    })();
    finish_transaction(conn, result)
}

fn canonicalize_usage_event_paths(conn: &Connection) -> Result<()> {
    let mut statement = conn.prepare(
        "SELECT rowid, project_path FROM ai_history_file_usage_event ORDER BY rowid;",
    )?;
    let rows = statement
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    for (rowid, project_path) in rows {
        let canonical_path = canonical_project_path(&project_path);
        if canonical_path.is_empty() || canonical_path == project_path {
            continue;
        }
        conn.execute(
            r#"
            INSERT OR IGNORE INTO ai_history_file_usage_event (
                source, file_path, project_path, project_id, event_ordinal,
                session_key, occurred_at, total_tokens, request_count,
                active_duration_seconds
            )
            SELECT source, file_path, ?1, project_id, event_ordinal,
                   session_key, occurred_at, total_tokens, request_count,
                   active_duration_seconds
            FROM ai_history_file_usage_event
            WHERE rowid = ?2;
            "#,
            params![canonical_path, rowid],
        )?;
        conn.execute(
            "DELETE FROM ai_history_file_usage_event WHERE rowid = ?1;",
            params![rowid],
        )?;
    }
    Ok(())
}

fn usage_event_schema_is_current(conn: &Connection) -> Result<bool> {
    let columns = usage_event_columns(conn)?;
    Ok(["project_id", "request_count", "active_duration_seconds"]
        .iter()
        .all(|name| columns.contains(*name)))
}

fn ensure_usage_event_columns(conn: &Connection) -> Result<()> {
    let columns = usage_event_columns(conn)?;
    for (name, definition) in [
        ("project_id", "TEXT NOT NULL DEFAULT ''"),
        ("request_count", "INTEGER NOT NULL DEFAULT 0"),
        ("active_duration_seconds", "INTEGER NOT NULL DEFAULT 0"),
    ] {
        if !columns.contains(name) {
            conn.execute_batch(&format!(
                "ALTER TABLE ai_history_file_usage_event ADD COLUMN {name} {definition};"
            ))?;
        }
    }
    Ok(())
}

fn usage_event_columns(conn: &Connection) -> Result<HashSet<String>> {
    let mut statement = conn.prepare("PRAGMA table_info(ai_history_file_usage_event);")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<HashSet<_>, _>>()?;
    Ok(columns)
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

    for metadata in &parsed.sessions {
        let session = sessions
            .entry(metadata.session_id.clone())
            .or_insert_with(|| ParsedSessionAccumulator {
                session_key: metadata.session_id.clone(),
                first_seen_at: metadata.timestamp,
                last_seen_at: metadata.timestamp,
                ..Default::default()
            });
        session.external_session_id = metadata
            .external_session_id
            .clone()
            .or(session.external_session_id.clone());
        session.title = metadata.session_title.clone().or(session.title.clone());
        session.last_model = metadata.model.clone().or(session.last_model.clone());
        session.first_seen_at = min_nonzero(session.first_seen_at, metadata.timestamp);
        session.last_seen_at = session.last_seen_at.max(metadata.timestamp);
    }

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
            active_duration_by_session
                .get(&event.session_id)
                .map(|duration| duration.total_seconds)
                .unwrap_or(0),
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
            active_duration_by_session
                .get(&entry.session_id)
                .map(|duration| duration.total_seconds)
                .unwrap_or(0),
        );
    }

    let mut buckets = HashMap::<(String, String, i64), AIUsageBucket>::new();
    let mut bucket_models = HashMap::<(String, i64), String>::new();
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
        bucket_models
            .entry((entry.session_id.clone(), bucket_start as i64))
            .or_insert_with(|| model.clone());
        bucket.input_tokens += entry.input_tokens;
        bucket.output_tokens += entry.output_tokens + entry.reasoning_output_tokens;
        bucket.total_tokens += entry.total_tokens();
        bucket.cached_input_tokens += entry.cached_input_tokens;
        merge_usage_amounts(&mut bucket.usage_amounts, &entry.usage_amounts);
    }

    for event in &parsed.events {
        if event.kind != HistoryEventKind::Request {
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
        bucket_models
            .entry((event.session_id.clone(), bucket_start as i64))
            .or_insert_with(|| model.clone());
        bucket.request_count += 1;
    }

    for (session_id, duration) in &active_duration_by_session {
        let Some(session) = sessions.get(session_id) else {
            continue;
        };
        for (&bucket_start, &seconds) in &duration.seconds_by_bucket {
            if seconds <= 0 {
                continue;
            }
            let model = bucket_models
                .get(&(session_id.clone(), bucket_start))
                .cloned()
                .or_else(|| session.last_model.clone())
                .unwrap_or_else(|| "unknown".to_string());
            let bucket = buckets
                .entry((session_id.clone(), model.clone(), bucket_start))
                .or_insert_with(|| {
                    usage_bucket_from_session(
                        source,
                        session,
                        project,
                        &model,
                        bucket_start as f64,
                    )
                });
            bucket.active_duration_seconds = bucket
                .active_duration_seconds
                .saturating_add(seconds);
        }
    }

    let mut usage_buckets = buckets.into_values().collect::<Vec<_>>();
    usage_buckets.sort_by(|left, right| {
        left.bucket_start
            .total_cmp(&right.bucket_start)
            .then_with(|| left.source.cmp(&right.source))
            .then_with(|| left.session_key.cmp(&right.session_key))
            .then_with(|| left.model.cmp(&right.model))
    });
    let mut usage_events = parsed
        .entries
        .iter()
        .filter_map(|entry| {
            let total_tokens = entry.total_tokens();
            (total_tokens > 0).then(|| AIUsageEvent {
                project_id: project.id.clone(),
                session_key: entry.session_id.clone(),
                occurred_at: entry.timestamp.floor() as i64,
                total_tokens,
                request_count: 0,
                active_duration_seconds: 0,
            })
        })
        .collect::<Vec<_>>();
    usage_events.extend(parsed.events.iter().filter_map(|event| {
        (event.kind == HistoryEventKind::Request).then(|| AIUsageEvent {
            project_id: project.id.clone(),
            session_key: event.session_id.clone(),
            occurred_at: event.timestamp.floor() as i64,
            total_tokens: 0,
            request_count: 1,
            active_duration_seconds: 0,
        })
    }));
    for (session_key, duration) in active_duration_by_session {
        usage_events.extend(duration.intervals.into_iter().filter_map(|(start, end)| {
            let start = start.round() as i64;
            let end = end.round() as i64;
            (end > start).then(|| AIUsageEvent {
                project_id: project.id.clone(),
                session_key: session_key.clone(),
                occurred_at: start,
                total_tokens: 0,
                request_count: 0,
                active_duration_seconds: end - start,
            })
        }));
    }
    usage_events.sort_by(|left, right| {
        left.occurred_at
            .cmp(&right.occurred_at)
            .then_with(|| left.session_key.cmp(&right.session_key))
    });

    AIExternalFileSummary {
        source: source.to_string(),
        file_path,
        file_modified_at,
        file_size,
        project_path: project.path.clone(),
        usage_buckets,
        usage_events,
    }
}
