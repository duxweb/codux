fn accumulate_breakdown(
    map: &mut HashMap<String, AIUsageBreakdownItem>,
    key: &str,
    total_tokens: i64,
    cached_input_tokens: i64,
    request_count: i64,
    usage_amounts: &[AIUsageAmount],
) {
    let item = map.entry(key.to_string()).or_insert(AIUsageBreakdownItem {
        key: key.to_string(),
        total_tokens: 0,
        cached_input_tokens: 0,
        request_count: 0,
        usage_amounts: Vec::new(),
    });
    item.total_tokens += total_tokens;
    item.cached_input_tokens += cached_input_tokens;
    item.request_count += request_count;
    merge_usage_amounts(&mut item.usage_amounts, usage_amounts);
}

fn sorted_breakdown(mut map: HashMap<String, AIUsageBreakdownItem>) -> Vec<AIUsageBreakdownItem> {
    let mut values = map.drain().map(|(_, value)| value).collect::<Vec<_>>();
    values.sort_by(|left, right| {
        right
            .total_tokens
            .cmp(&left.total_tokens)
            .then_with(|| {
                right
                    .usage_amounts
                    .iter()
                    .map(|amount| amount.value)
                    .sum::<f64>()
                    .total_cmp(
                        &left
                            .usage_amounts
                            .iter()
                            .map(|amount| amount.value)
                            .sum::<f64>(),
                    )
            })
    });
    values
}

fn merge_usage_amount(amounts: &mut Vec<AIUsageAmount>, next: AIUsageAmount) {
    if next.unit.trim().is_empty() || next.value <= 0.0 {
        return;
    }
    if let Some(existing) = amounts.iter_mut().find(|item| item.unit == next.unit) {
        existing.value += next.value;
    } else {
        amounts.push(next);
    }
}

fn merge_usage_amounts(amounts: &mut Vec<AIUsageAmount>, next: &[AIUsageAmount]) {
    for amount in next {
        merge_usage_amount(amounts, amount.clone());
    }
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
                input_tokens: 0,
                output_tokens: 0,
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

fn merge_logical_sessions(sessions: Vec<AISessionSummary>) -> Vec<AISessionSummary> {
    let mut merged = HashMap::<(String, String), AISessionSummary>::new();
    for session in sessions {
        let key = (session.session_id.clone(), session.project_path.clone());
        if let Some(current) = merged.get_mut(&key) {
            let newer = session.last_seen_at >= current.last_seen_at;
            current.first_seen_at = min_nonzero(current.first_seen_at, session.first_seen_at);
            current.last_seen_at = current.last_seen_at.max(session.last_seen_at);
            current.request_count = current.request_count.saturating_add(session.request_count);
            current.total_input_tokens = current
                .total_input_tokens
                .saturating_add(session.total_input_tokens);
            current.total_output_tokens = current
                .total_output_tokens
                .saturating_add(session.total_output_tokens);
            current.total_tokens = current.total_tokens.saturating_add(session.total_tokens);
            current.cached_input_tokens = current
                .cached_input_tokens
                .saturating_add(session.cached_input_tokens);
            current.active_duration_seconds = current
                .active_duration_seconds
                .max(session.active_duration_seconds);
            current.today_tokens = current.today_tokens.saturating_add(session.today_tokens);
            current.today_cached_input_tokens = current
                .today_cached_input_tokens
                .saturating_add(session.today_cached_input_tokens);
            if newer {
                current.project_id = session.project_id;
                current.project_name = session.project_name;
                current.session_title = session.session_title;
                current.last_tool = session.last_tool;
                current.last_model = session.last_model;
            }
        } else {
            merged.insert(key, session);
        }
    }
    let mut sessions = merged.into_values().collect::<Vec<_>>();
    sessions.sort_by(|left, right| right.last_seen_at.total_cmp(&left.last_seen_at));
    sessions
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
    codux_runtime_core::path::normalize_local_path(path)
}

// Single canonical form for every project_path written to or queried from the
// store; keeps /var vs /private/var (and other alias) identities consistent.
pub(crate) fn canonical_project_path(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        return String::new();
    }
    normalized_path(Path::new(value))
}

fn canonical_project_request(mut project: AIHistoryProjectRequest) -> AIHistoryProjectRequest {
    project.path = canonical_project_path(&project.path);
    project
}

fn modified_seconds(metadata: &fs::Metadata) -> f64 {
    metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(duration_seconds)
        .unwrap_or(0.0)
}

fn history_file_version(path: &Path, metadata: &fs::Metadata) -> (f64, i64) {
    let mut modified_at = modified_seconds(metadata);
    let mut file_size = metadata.len().min(i64::MAX as u64) as i64;
    if is_sqlite_history_database_path(path) {
        for sidecar in [path_with_suffix(path, "-wal"), path_with_suffix(path, "-shm")] {
            let Ok(metadata) = fs::metadata(sidecar) else {
                continue;
            };
            modified_at = modified_at.max(modified_seconds(&metadata));
            file_size = file_size.saturating_add(metadata.len().min(i64::MAX as u64) as i64);
        }
    }
    (modified_at, file_size)
}

fn is_sqlite_history_database_path(path: &Path) -> bool {
    path.extension().and_then(|value| value.to_str()) == Some("db")
        || path.file_name().and_then(|value| value.to_str()) == Some("state_5.sqlite")
}

fn path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{}", path.display(), suffix))
}

fn duration_seconds(duration: std::time::Duration) -> f64 {
    duration.as_secs() as f64 + f64::from(duration.subsec_micros()) / 1_000_000.0
}

fn default_database_path() -> PathBuf {
    app_support_dir().join("ai-usage.sqlite3")
}
