impl ParsedHistory {
    fn merge(&mut self, other: ParsedHistory) {
        self.entries.extend(other.entries);
        self.events.extend(other.events);
    }
}

fn build_snapshot(project: AIHistoryProjectRequest, parsed: ParsedHistory) -> AIHistorySnapshot {
    let today_start = local_day_start_seconds(now_seconds());
    let active_duration_by_key = active_duration_by_history_key(&parsed.events);
    let mut sessions_by_key: HashMap<String, SessionAccumulator> = HashMap::new();

    for event in &parsed.events {
        let key = history_key(&event.source, &event.session_id);
        let active_duration = *active_duration_by_key.get(&key).unwrap_or(&0);
        let session = sessions_by_key
            .entry(key)
            .or_insert_with(|| SessionAccumulator {
                source: event.source.clone(),
                session_id: event.session_id.clone(),
                first_seen_at: event.timestamp,
                last_seen_at: event.timestamp,
                ..Default::default()
            });
        session.first_seen_at = min_nonzero(session.first_seen_at, event.timestamp);
        session.last_seen_at = session.last_seen_at.max(event.timestamp);
        session.active_duration_seconds = session.active_duration_seconds.max(active_duration);
        if event.role == HistoryRole::User {
            session.request_count += 1;
        }
    }

    let mut tool_breakdown: HashMap<String, AIUsageBreakdownItem> = HashMap::new();
    let mut model_breakdown: HashMap<String, AIUsageBreakdownItem> = HashMap::new();
    let mut heatmap: HashMap<i64, AIHeatmapDay> = HashMap::new();
    let mut time_buckets: HashMap<i64, AITimeBucket> = HashMap::new();
    let mut project_total_tokens = 0;
    let mut project_cached_input_tokens = 0;
    let mut today_total_tokens = 0;
    let mut today_cached_input_tokens = 0;

    for entry in &parsed.entries {
        let total_tokens = entry.total_tokens();
        let key = history_key(&entry.source, &entry.session_id);
        let active_duration = *active_duration_by_key.get(&key).unwrap_or(&0);
        let session = sessions_by_key
            .entry(key)
            .or_insert_with(|| SessionAccumulator {
                source: entry.source.clone(),
                session_id: entry.session_id.clone(),
                first_seen_at: entry.timestamp,
                last_seen_at: entry.timestamp,
                ..Default::default()
            });
        session.external_session_id = entry
            .external_session_id
            .clone()
            .or(session.external_session_id.clone());
        session.title = entry.session_title.clone().or(session.title.clone());
        session.model = entry.model.clone().or(session.model.clone());
        session.first_seen_at = min_nonzero(session.first_seen_at, entry.timestamp);
        session.last_seen_at = session.last_seen_at.max(entry.timestamp);
        session.input_tokens += entry.input_tokens;
        session.output_tokens += entry.output_tokens;
        session.cached_input_tokens += entry.cached_input_tokens;
        session.reasoning_output_tokens += entry.reasoning_output_tokens;
        session.active_duration_seconds = session.active_duration_seconds.max(active_duration);
        if entry.timestamp >= today_start {
            session.today_tokens += total_tokens;
            session.today_cached_input_tokens += entry.cached_input_tokens;
        }

        project_total_tokens += total_tokens;
        project_cached_input_tokens += entry.cached_input_tokens;
        if entry.timestamp >= today_start {
            today_total_tokens += total_tokens;
            today_cached_input_tokens += entry.cached_input_tokens;
        }

        accumulate_breakdown(
            &mut tool_breakdown,
            &entry.source,
            total_tokens,
            entry.cached_input_tokens,
        );
        if let Some(model) = displayable_model_name(entry.model.as_deref()) {
            accumulate_breakdown(
                &mut model_breakdown,
                model,
                total_tokens,
                entry.cached_input_tokens,
            );
        }

        let day = local_day_start_seconds(entry.timestamp);
        let day_key = day as i64;
        let day_item = heatmap.entry(day_key).or_insert(AIHeatmapDay {
            day,
            total_tokens: 0,
            cached_input_tokens: 0,
            request_count: 0,
        });
        day_item.total_tokens += total_tokens;
        day_item.cached_input_tokens += entry.cached_input_tokens;

        if entry.timestamp >= today_start {
            let bucket_start = half_hour_bucket_start(entry.timestamp);
            let bucket = time_buckets
                .entry(bucket_start as i64)
                .or_insert(AITimeBucket {
                    start: bucket_start,
                    end: bucket_start + 30.0 * 60.0,
                    total_tokens: 0,
                    cached_input_tokens: 0,
                    request_count: 0,
                });
            bucket.total_tokens += total_tokens;
            bucket.cached_input_tokens += entry.cached_input_tokens;
        }
    }

    for event in &parsed.events {
        let day = local_day_start_seconds(event.timestamp);
        if event.role == HistoryRole::User {
            if let Some(day_item) = heatmap.get_mut(&(day as i64)) {
                day_item.request_count += 1;
            } else {
                heatmap.insert(
                    day as i64,
                    AIHeatmapDay {
                        day,
                        total_tokens: 0,
                        cached_input_tokens: 0,
                        request_count: 1,
                    },
                );
            }
            if event.timestamp >= today_start {
                let bucket_start = half_hour_bucket_start(event.timestamp);
                let bucket = time_buckets
                    .entry(bucket_start as i64)
                    .or_insert(AITimeBucket {
                        start: bucket_start,
                        end: bucket_start + 30.0 * 60.0,
                        total_tokens: 0,
                        cached_input_tokens: 0,
                        request_count: 0,
                    });
                bucket.request_count += 1;
            }
            let tool_key = event.source.clone();
            if let Some(item) = tool_breakdown.get_mut(&tool_key) {
                item.request_count += 1;
            }
        }
    }

    let mut sessions = sessions_by_key
        .into_values()
        .filter(|session| {
            session.input_tokens
                + session.output_tokens
                + session.reasoning_output_tokens
                + session.request_count
                > 0
        })
        .map(|session| {
            let total_tokens =
                session.input_tokens + session.output_tokens + session.reasoning_output_tokens;
            AISessionSummary {
                session_id: deterministic_uuid(&history_key(&session.source, &session.session_id)),
                external_session_id: session.external_session_id,
                project_id: project.id.clone(),
                project_name: project.name.clone(),
                project_path: project.path.clone(),
                session_title: session.title.unwrap_or_else(|| project.name.clone()),
                first_seen_at: session.first_seen_at,
                last_seen_at: session.last_seen_at,
                last_tool: Some(session.source),
                last_model: session.model,
                request_count: session.request_count,
                total_input_tokens: session.input_tokens,
                total_output_tokens: session.output_tokens,
                total_tokens,
                cached_input_tokens: session.cached_input_tokens,
                active_duration_seconds: session.active_duration_seconds.min(
                    (session.last_seen_at - session.first_seen_at)
                        .max(0.0)
                        .round() as i64,
                ),
                today_tokens: session.today_tokens,
                today_cached_input_tokens: session.today_cached_input_tokens,
            }
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
