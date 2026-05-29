fn opencode_tokens_usage(value: &Value) -> HistoryUsage {
    let cache = value.get("cache").unwrap_or(&Value::Null);
    HistoryUsage {
        input_tokens: json_i64(value.get("input")),
        output_tokens: json_i64(value.get("output")),
        cached_input_tokens: json_i64(cache.get("read")),
        reasoning_output_tokens: json_i64(value.get("reasoning")),
    }
}

#[derive(Debug, Clone)]
struct HistoryUsage {
    input_tokens: i64,
    output_tokens: i64,
    cached_input_tokens: i64,
    reasoning_output_tokens: i64,
}

impl HistoryUsage {
    fn total_tokens(&self) -> i64 {
        self.input_tokens + self.output_tokens + self.reasoning_output_tokens
    }

    fn delta(&self, delta: i64) -> Self {
        if delta <= 0 || self.total_tokens() <= 0 {
            return Self {
                input_tokens: delta.max(0),
                output_tokens: 0,
                cached_input_tokens: 0,
                reasoning_output_tokens: 0,
            };
        }
        let ratio = delta as f64 / self.total_tokens() as f64;
        let output = (self.output_tokens as f64 * ratio).round() as i64;
        let reasoning = (self.reasoning_output_tokens as f64 * ratio).round() as i64;
        let cached = (self.cached_input_tokens as f64 * ratio).round() as i64;
        Self {
            input_tokens: (delta - output - reasoning).max(0),
            output_tokens: output.max(0),
            cached_input_tokens: cached.max(0),
            reasoning_output_tokens: reasoning.max(0),
        }
    }
}

fn codex_history_usage(value: Option<&Value>) -> Option<HistoryUsage> {
    let value = value?;
    let cached_input_tokens =
        json_i64(value.get("cached_input_tokens")) + json_i64(value.get("cache_read_input_tokens"));
    let reasoning_output_tokens = json_i64(value.get("reasoning_output_tokens"));
    let input_tokens = (json_i64(value.get("input_tokens")) - cached_input_tokens).max(0);
    let output_tokens = (json_i64(value.get("output_tokens")) - reasoning_output_tokens).max(0);
    let usage = HistoryUsage {
        input_tokens,
        output_tokens,
        cached_input_tokens,
        reasoning_output_tokens,
    };
    (usage.total_tokens() > 0 || usage.cached_input_tokens > 0).then_some(usage)
}

fn gemini_tokens_usage(value: &Value) -> HistoryUsage {
    let cached = json_i64(value.get("cached"));
    let reasoning = json_i64(value.get("thoughts"));
    HistoryUsage {
        input_tokens: (json_i64(value.get("input")) - cached).max(0),
        output_tokens: (json_i64(value.get("output")) - reasoning).max(0),
        cached_input_tokens: cached.max(0),
        reasoning_output_tokens: reasoning.max(0),
    }
}

fn gemini_usage_metadata(value: &Value) -> HistoryUsage {
    let cached = json_i64(value.get("cachedContentTokenCount"));
    let reasoning = json_i64(value.get("thoughtsTokenCount"));
    HistoryUsage {
        input_tokens: (json_i64(value.get("promptTokenCount"))
            + json_i64(value.get("input_tokens"))
            - cached)
            .max(0),
        output_tokens: (json_i64(value.get("candidatesTokenCount"))
            + json_i64(value.get("output_tokens"))
            - reasoning)
            .max(0),
        cached_input_tokens: cached.max(0),
        reasoning_output_tokens: reasoning.max(0),
    }
}

fn accumulate_breakdown(
    map: &mut HashMap<String, AIUsageBreakdownItem>,
    key: &str,
    total_tokens: i64,
    cached_input_tokens: i64,
) {
    let item = map.entry(key.to_string()).or_insert(AIUsageBreakdownItem {
        key: key.to_string(),
        total_tokens: 0,
        cached_input_tokens: 0,
        request_count: 0,
    });
    item.total_tokens += total_tokens;
    item.cached_input_tokens += cached_input_tokens;
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

fn active_duration_by_history_key(events: &[HistoryEvent]) -> HashMap<String, i64> {
    let mut grouped = HashMap::<String, Vec<&HistoryEvent>>::new();
    for event in events {
        grouped
            .entry(history_key(&event.source, &event.session_id))
            .or_default()
            .push(event);
    }

    let mut result = HashMap::new();
    for (key, mut events) in grouped {
        events.sort_by(|left, right| left.timestamp.total_cmp(&right.timestamp));
        let Some(first) = events.first() else {
            continue;
        };
        let Some(last) = events.last() else {
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
        result.insert(key, active_seconds.min(wall_clock_seconds));
    }
    result
}

pub(crate) fn history_key(source: &str, session_id: &str) -> String {
    format!("{source}:{session_id}")
}

pub(crate) fn deterministic_uuid(value: &str) -> String {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, value.as_bytes()).to_string()
}

fn min_nonzero(left: f64, right: f64) -> f64 {
    if left <= 0.0 { right } else { left.min(right) }
}

pub(crate) fn half_hour_bucket_start(timestamp: f64) -> f64 {
    let Some(date) = Local.timestamp_opt(timestamp as i64, 0).single() else {
        return timestamp;
    };
    let minute = if date.minute() < 30 { 0 } else { 30 };
    Local
        .with_ymd_and_hms(
            date.year(),
            date.month(),
            date.day(),
            date.hour(),
            minute,
            0,
        )
        .single()
        .map(|date| date.timestamp() as f64)
        .unwrap_or(timestamp)
}

pub fn local_day_start_seconds(timestamp: f64) -> f64 {
    let Some(date) = Local.timestamp_opt(timestamp as i64, 0).single() else {
        return timestamp;
    };
    Local
        .with_ymd_and_hms(date.year(), date.month(), date.day(), 0, 0, 0)
        .single()
        .map(|date| date.timestamp() as f64)
        .unwrap_or(timestamp)
}

pub(crate) fn now_seconds() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}
