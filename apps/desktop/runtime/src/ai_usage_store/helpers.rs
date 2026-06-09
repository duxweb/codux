fn accumulate_breakdown(
    map: &mut HashMap<String, AIUsageBreakdownItem>,
    key: &str,
    total_tokens: i64,
    cached_input_tokens: i64,
    request_count: i64,
) {
    let item = map.entry(key.to_string()).or_insert(AIUsageBreakdownItem {
        key: key.to_string(),
        total_tokens: 0,
        cached_input_tokens: 0,
        request_count: 0,
    });
    item.total_tokens += total_tokens;
    item.cached_input_tokens += cached_input_tokens;
    item.request_count += request_count;
}

fn sorted_breakdown(mut map: HashMap<String, AIUsageBreakdownItem>) -> Vec<AIUsageBreakdownItem> {
    let mut values = map.drain().map(|(_, value)| value).collect::<Vec<_>>();
    values.sort_by(|left, right| right.total_tokens.cmp(&left.total_tokens));
    values
}

fn sorted_values<T>(map: HashMap<i64, T>) -> Vec<T> {
    let mut entries = map.into_iter().collect::<Vec<_>>();
    entries.sort_by_key(|(key, _)| *key);
    entries.into_iter().map(|(_, value)| value).collect()
}

fn fixed_today_time_buckets(mut map: HashMap<i64, AITimeBucket>) -> Vec<AITimeBucket> {
    let today_start = local_day_start_seconds(now_seconds());
    (0..48)
        .map(|index| {
            let start = today_start + f64::from(index) * 30.0 * 60.0;
            map.remove(&(start as i64)).unwrap_or(AITimeBucket {
                start,
                end: start + 30.0 * 60.0,
                total_tokens: 0,
                cached_input_tokens: 0,
                request_count: 0,
            })
        })
        .collect()
}

fn matching_session_keys(
    links: &[NormalizedSessionLinkRow],
    session_id: &str,
) -> Vec<(String, String)> {
    let mut matched = Vec::new();
    for link in links {
        let raw_id = deterministic_uuid(&history_key(&link.source, &link.session_key));
        let grouped_id = deterministic_uuid(&history_group_key(
            &link.source,
            &link.session_key,
            link.external_session_id.as_deref(),
        ));
        if session_id == raw_id || session_id == grouped_id {
            let key = (link.source.clone(), link.session_key.clone());
            if !matched.contains(&key) {
                matched.push(key);
            }
        }
    }
    matched
}

fn history_group_key(source: &str, session_key: &str, external_session_id: Option<&str>) -> String {
    history_key(source, external_session_id.unwrap_or(session_key))
}

fn active_duration_by_session_id(events: &[HistoryEvent]) -> HashMap<String, i64> {
    let mut grouped = HashMap::<String, Vec<&HistoryEvent>>::new();
    for event in events {
        grouped
            .entry(event.session_id.clone())
            .or_default()
            .push(event);
    }

    let mut result = HashMap::new();
    for (session_id, mut events) in grouped {
        events.sort_by(|left, right| left.timestamp.total_cmp(&right.timestamp));
        let (Some(first), Some(last)) = (events.first(), events.last()) else {
            continue;
        };
        let wall_clock_seconds = (last.timestamp - first.timestamp).max(0.0).round() as i64;
        let mut active_seconds = 0i64;
        let mut waiting_for_first_response = false;
        let mut turn_start: Option<f64> = None;
        let mut turn_end: Option<f64> = None;

        for event in events {
            match event.role {
                HistoryRole::User => {
                    if let (Some(start), Some(end)) = (turn_start, turn_end) {
                        if end > start {
                            active_seconds = active_seconds
                                .saturating_add((end - start).max(0.0).round() as i64)
                                .min(wall_clock_seconds);
                        }
                    }
                    turn_start = None;
                    turn_end = None;
                    waiting_for_first_response = true;
                }
                HistoryRole::Assistant => {
                    if waiting_for_first_response {
                        turn_start = Some(event.timestamp);
                        turn_end = Some(event.timestamp);
                        waiting_for_first_response = false;
                    } else if turn_start.is_some() {
                        turn_end = Some(event.timestamp);
                    }
                }
            }
        }
        if let (Some(start), Some(end)) = (turn_start, turn_end) {
            if end > start {
                active_seconds = active_seconds
                    .saturating_add((end - start).max(0.0).round() as i64)
                    .min(wall_clock_seconds);
            }
        }
        result.insert(session_id, active_seconds.min(wall_clock_seconds));
    }
    result
}

fn min_nonzero(left: f64, right: f64) -> f64 {
    if left <= 0.0 { right } else { left.min(right) }
}

fn preferred_string(left: Option<&str>, right: Option<&str>) -> Option<String> {
    normalized_optional_string(left.unwrap_or(""))
        .or_else(|| normalized_optional_string(right.unwrap_or("")))
}

fn normalized_optional_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn displayable_model_name(value: Option<&str>) -> Option<&str> {
    let value = value?.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("unknown") {
        return None;
    }
    Some(value)
}

fn same_timestamp(left: f64, right: f64) -> bool {
    (left - right).abs() < 0.000_001
}

fn normalized_path(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

fn modified_seconds(metadata: &fs::Metadata) -> f64 {
    metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(duration_seconds)
        .unwrap_or(0.0)
}

fn duration_seconds(duration: std::time::Duration) -> f64 {
    duration.as_secs() as f64 + f64::from(duration.subsec_micros()) / 1_000_000.0
}

fn default_database_path() -> PathBuf {
    app_support_dir().join("ai-usage.sqlite3")
}
