fn initialize_connection(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;

    for statement in SCHEMA_STATEMENTS {
        conn.execute_batch(statement)?;
    }

    let stored_version: Option<String> = conn
        .query_row(
            "SELECT value FROM ai_history_meta WHERE key = 'normalized_history_schema_version' LIMIT 1;",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if stored_version.as_deref() != Some(NORMALIZED_HISTORY_SCHEMA_VERSION) {
        migrate_schema(conn)?;
    }
    Ok(())
}

fn jsonl_index_mode(
    current_file_size: i64,
    current_modified_at: f64,
    stored_summary: Option<&AIExternalFileSummary>,
    checkpoint: Option<&AIExternalFileCheckpoint>,
) -> JSONLIndexMode {
    let (Some(stored_summary), Some(checkpoint)) = (stored_summary, checkpoint) else {
        return JSONLIndexMode::Rebuild;
    };
    if current_file_size < checkpoint.file_size {
        return JSONLIndexMode::Rebuild;
    }
    if checkpoint.last_offset < current_file_size {
        return JSONLIndexMode::Append;
    }
    if same_timestamp(stored_summary.file_modified_at, current_modified_at)
        && same_timestamp(checkpoint.file_modified_at, current_modified_at)
        && checkpoint.last_offset >= current_file_size
    {
        return JSONLIndexMode::Unchanged;
    }
    if current_file_size >= checkpoint.file_size && checkpoint.last_offset <= current_file_size {
        return JSONLIndexMode::Append;
    }
    JSONLIndexMode::Rebuild
}

fn merge_usage_buckets(existing: &[AIUsageBucket], delta: &[AIUsageBucket]) -> Vec<AIUsageBucket> {
    let mut map = HashMap::<(String, String, String, i64), AIUsageBucket>::new();
    for bucket in existing.iter().chain(delta.iter()) {
        let key = (
            bucket.source.clone(),
            bucket.session_key.clone(),
            bucket.model.clone().unwrap_or_default(),
            bucket.bucket_start as i64,
        );
        map.entry(key)
            .and_modify(|current| {
                current.input_tokens += bucket.input_tokens;
                current.output_tokens += bucket.output_tokens;
                current.total_tokens += bucket.total_tokens;
                current.cached_input_tokens += bucket.cached_input_tokens;
                current.request_count += bucket.request_count;
                current.active_duration_seconds += bucket.active_duration_seconds;
                current.first_seen_at = min_nonzero(current.first_seen_at, bucket.first_seen_at);
                current.last_seen_at = current.last_seen_at.max(bucket.last_seen_at);
                current.external_session_id = current
                    .external_session_id
                    .clone()
                    .or(bucket.external_session_id.clone());
                current.session_title =
                    preferred_string(Some(&current.session_title), Some(&bucket.session_title))
                        .unwrap_or_else(|| bucket.project_name.clone());
                current.model = current.model.clone().or(bucket.model.clone());
            })
            .or_insert_with(|| bucket.clone());
    }
    let mut values = map.into_values().collect::<Vec<_>>();
    values.sort_by(|left, right| {
        left.bucket_start
            .total_cmp(&right.bucket_start)
            .then_with(|| left.source.cmp(&right.source))
            .then_with(|| left.session_key.cmp(&right.session_key))
            .then_with(|| left.model.cmp(&right.model))
    });
    values
}
