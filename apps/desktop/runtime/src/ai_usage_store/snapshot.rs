fn build_snapshot_from_rows(
    project: AIHistoryProjectRequest,
    links: Vec<NormalizedSessionLinkRow>,
    buckets: Vec<StoredUsageBucketRow>,
) -> AIHistorySnapshot {
    let today_start = local_day_start_seconds(now_seconds());
    let mut sessions_by_key = HashMap::<String, PersistedSessionAccumulator>::new();
    let mut tool_breakdown = HashMap::<String, AIUsageBreakdownItem>::new();
    let mut model_breakdown = HashMap::<String, AIUsageBreakdownItem>::new();
    let mut heatmap = HashMap::<i64, AIHeatmapDay>::new();
    let mut time_buckets = HashMap::<i64, AITimeBucket>::new();
    let mut project_total_tokens = 0;
    let mut project_cached_input_tokens = 0;
    let mut today_total_tokens = 0;
    let mut today_cached_input_tokens = 0;
    let link_group_keys = links
        .iter()
        .map(|link| {
            (
                history_key(&link.source, &link.session_key),
                history_group_key(
                    &link.source,
                    &link.session_key,
                    link.external_session_id.as_deref(),
                ),
            )
        })
        .collect::<HashMap<_, _>>();

    for link in &links {
        let key = history_group_key(
            &link.source,
            &link.session_key,
            link.external_session_id.as_deref(),
        );
        let session = sessions_by_key
            .entry(key)
            .or_insert_with(|| PersistedSessionAccumulator {
                source: link.source.clone(),
                session_key: link.session_key.clone(),
                external_session_id: link.external_session_id.clone(),
                title: Some(link.session_title.clone()),
                first_seen_at: link.first_seen_at,
                last_seen_at: link.last_seen_at,
                last_model: link.last_model.clone(),
                active_duration_seconds: link.active_duration_seconds,
                ..Default::default()
            });
        session.external_session_id = session
            .external_session_id
            .clone()
            .or(link.external_session_id.clone());
        session.title = preferred_string(session.title.as_deref(), Some(&link.session_title));
        session.first_seen_at = min_nonzero(session.first_seen_at, link.first_seen_at);
        session.last_seen_at = session.last_seen_at.max(link.last_seen_at);
        if link.last_seen_at >= session.last_seen_at {
            session.last_model = link.last_model.clone().or(session.last_model.clone());
        }
        session.active_duration_seconds = session
            .active_duration_seconds
            .max(link.active_duration_seconds);
    }

    for bucket in buckets {
        let raw_key = history_key(&bucket.source, &bucket.session_key);
        let key = link_group_keys
            .get(&raw_key)
            .cloned()
            .unwrap_or_else(|| raw_key.clone());
        let session = sessions_by_key
            .entry(key)
            .or_insert_with(|| PersistedSessionAccumulator {
                source: bucket.source.clone(),
                session_key: bucket.session_key.clone(),
                first_seen_at: bucket.bucket_start,
                last_seen_at: bucket.bucket_end,
                ..Default::default()
            });
        session.input_tokens += bucket.input_tokens;
        session.output_tokens += bucket.output_tokens;
        session.total_tokens += bucket.total_tokens;
        session.cached_input_tokens += bucket.cached_input_tokens;
        session.request_count += bucket.request_count;
        session.first_seen_at = min_nonzero(session.first_seen_at, bucket.bucket_start);
        session.last_seen_at = session.last_seen_at.max(bucket.bucket_end);
        session.last_model = bucket.model.clone().or(session.last_model.clone());
        if bucket.bucket_start >= today_start {
            session.today_tokens += bucket.total_tokens;
            session.today_cached_input_tokens += bucket.cached_input_tokens;
        }

        project_total_tokens += bucket.total_tokens;
        project_cached_input_tokens += bucket.cached_input_tokens;
        if bucket.bucket_start >= today_start {
            today_total_tokens += bucket.total_tokens;
            today_cached_input_tokens += bucket.cached_input_tokens;
        }

        accumulate_breakdown(
            &mut tool_breakdown,
            &bucket.source,
            bucket.total_tokens,
            bucket.cached_input_tokens,
            bucket.request_count,
        );
        if let Some(model) = displayable_model_name(bucket.model.as_deref()) {
            accumulate_breakdown(
                &mut model_breakdown,
                model,
                bucket.total_tokens,
                bucket.cached_input_tokens,
                bucket.request_count,
            );
        }

        let day = local_day_start_seconds(bucket.bucket_start);
        let heatmap_day = heatmap.entry(day as i64).or_insert(AIHeatmapDay {
            day,
            total_tokens: 0,
            cached_input_tokens: 0,
            request_count: 0,
        });
        heatmap_day.total_tokens += bucket.total_tokens;
        heatmap_day.cached_input_tokens += bucket.cached_input_tokens;
        heatmap_day.request_count += bucket.request_count;

        if bucket.bucket_start >= today_start {
            let item = time_buckets
                .entry(bucket.bucket_start as i64)
                .or_insert(AITimeBucket {
                    start: bucket.bucket_start,
                    end: bucket.bucket_end,
                    total_tokens: 0,
                    cached_input_tokens: 0,
                    request_count: 0,
                });
            item.total_tokens += bucket.total_tokens;
            item.cached_input_tokens += bucket.cached_input_tokens;
            item.request_count += bucket.request_count;
        }
    }

    let mut sessions = sessions_by_key
        .into_values()
        .filter(|session| {
            session.total_tokens + session.cached_input_tokens + session.request_count > 0
        })
        .map(|session| AISessionSummary {
            session_id: deterministic_uuid(&history_key(&session.source, &session.session_key)),
            external_session_id: session.external_session_id,
            project_id: project.id.clone(),
            project_name: project.name.clone(),
            project_path: project.path.clone(),
            session_title: session.title.unwrap_or_else(|| project.name.clone()),
            first_seen_at: session.first_seen_at,
            last_seen_at: session.last_seen_at,
            last_tool: Some(session.source),
            last_model: session.last_model,
            request_count: session.request_count,
            total_input_tokens: session.input_tokens,
            total_output_tokens: session.output_tokens,
            total_tokens: session.total_tokens,
            cached_input_tokens: session.cached_input_tokens,
            active_duration_seconds: session.active_duration_seconds.max(
                (session.last_seen_at - session.first_seen_at)
                    .max(0.0)
                    .round() as i64,
            ),
            today_tokens: session.today_tokens,
            today_cached_input_tokens: session.today_cached_input_tokens,
        })
        .collect::<Vec<_>>();
    sessions.sort_by(|left, right| right.last_seen_at.total_cmp(&left.last_seen_at));

    let latest_session = sessions.first().cloned();
    sessions.truncate(RECENT_HISTORY_SESSION_LIMIT);
    AIHistorySnapshot {
        project_id: project.id.clone(),
        project_name: project.name.clone(),
        project_summary: AIProjectUsageSummary {
            project_id: project.id,
            project_name: project.name,
            current_session_tokens: latest_session
                .as_ref()
                .map(|session| session.total_tokens)
                .unwrap_or(0),
            current_session_cached_input_tokens: latest_session
                .as_ref()
                .map(|session| session.cached_input_tokens)
                .unwrap_or(0),
            project_total_tokens,
            project_cached_input_tokens,
            today_total_tokens,
            today_cached_input_tokens,
            current_tool: latest_session
                .as_ref()
                .and_then(|session| session.last_tool.clone()),
            current_model: latest_session
                .as_ref()
                .and_then(|session| session.last_model.clone()),
            current_session_updated_at: latest_session.as_ref().map(|session| session.last_seen_at),
        },
        sessions,
        heatmap: sorted_values(heatmap),
        today_time_buckets: fixed_today_time_buckets(time_buckets),
        tool_breakdown: sorted_breakdown(tool_breakdown),
        model_breakdown: sorted_breakdown(model_breakdown),
        indexed_at: now_seconds(),
    }
}
