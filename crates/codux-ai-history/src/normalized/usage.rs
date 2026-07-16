fn opencode_tokens_usage(value: &Value) -> HistoryUsage {
    let cache = value.get("cache").unwrap_or(&Value::Null);
    HistoryUsage {
        input_tokens: json_i64(value.get("input")),
        output_tokens: json_i64(value.get("output")),
        cached_input_tokens: json_i64(cache.get("read")),
        reasoning_output_tokens: json_i64(value.get("reasoning")),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct HistoryUsage {
    input_tokens: i64,
    output_tokens: i64,
    cached_input_tokens: i64,
    reasoning_output_tokens: i64,
}

impl HistoryUsage {
    fn total_tokens(&self) -> i64 {
        self.input_tokens + self.output_tokens + self.reasoning_output_tokens
    }

    fn cumulative_total_tokens(&self) -> i64 {
        self.total_tokens() + self.cached_input_tokens
    }

    fn saturating_delta(&self, previous: &Self) -> Self {
        Self {
            input_tokens: (self.input_tokens - previous.input_tokens).max(0),
            output_tokens: (self.output_tokens - previous.output_tokens).max(0),
            cached_input_tokens: (self.cached_input_tokens - previous.cached_input_tokens).max(0),
            reasoning_output_tokens: (self.reasoning_output_tokens
                - previous.reasoning_output_tokens)
                .max(0),
        }
    }

    fn componentwise_max(&self, previous: &Self) -> Self {
        Self {
            input_tokens: self.input_tokens.max(previous.input_tokens),
            output_tokens: self.output_tokens.max(previous.output_tokens),
            cached_input_tokens: self
                .cached_input_tokens
                .max(previous.cached_input_tokens),
            reasoning_output_tokens: self
                .reasoning_output_tokens
                .max(previous.reasoning_output_tokens),
        }
    }
}

fn codex_history_usage(value: Option<&Value>) -> Option<HistoryUsage> {
    let value = value?;
    let cached_input_tokens = json_i64(value.get("cached_input_tokens"))
        .max(json_i64(value.get("cache_read_input_tokens")));
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

fn accumulate_breakdown(
    map: &mut HashMap<String, AIUsageBreakdownItem>,
    key: &str,
    total_tokens: i64,
    cached_input_tokens: i64,
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
    merge_usage_amounts(&mut item.usage_amounts, usage_amounts);
}

fn sorted_breakdown(mut map: HashMap<String, AIUsageBreakdownItem>) -> Vec<AIUsageBreakdownItem> {
    let mut values = map.drain().map(|(_, value)| value).collect::<Vec<_>>();
    values.sort_by(|left, right| right.total_tokens.cmp(&left.total_tokens));
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

#[derive(Debug, Clone, Default)]
pub(crate) struct ActiveDurationSummary {
    pub(crate) total_seconds: i64,
    pub(crate) seconds_by_bucket: HashMap<i64, i64>,
    pub(crate) intervals: Vec<(f64, f64)>,
}

fn active_duration_by_history_key(
    events: &[HistoryEvent],
) -> HashMap<String, ActiveDurationSummary> {
    active_duration_by_key(events, |event| {
        history_key(&event.source, &event.session_id)
    })
}

pub(crate) fn active_duration_by_session_id(
    events: &[HistoryEvent],
) -> HashMap<String, ActiveDurationSummary> {
    active_duration_by_key(events, |event| event.session_id.clone())
}

fn active_duration_by_key(
    events: &[HistoryEvent],
    key_for_event: impl Fn(&HistoryEvent) -> String,
) -> HashMap<String, ActiveDurationSummary> {
    let mut grouped = HashMap::<String, Vec<&HistoryEvent>>::new();
    for event in events {
        grouped.entry(key_for_event(event)).or_default().push(event);
    }

    let mut result = HashMap::new();
    for (key, mut events) in grouped {
        events.sort_by(|left, right| left.timestamp.total_cmp(&right.timestamp));
        let intervals = if events.iter().any(|event| {
            matches!(
                event.kind,
                HistoryEventKind::ActivityStart | HistoryEventKind::ActivityEnd
            )
        }) {
            explicit_activity_intervals(&events)
        } else {
            conversational_activity_intervals(&events)
        };
        result.insert(key, summarize_activity_intervals(intervals));
    }
    result
}

fn explicit_activity_intervals(events: &[&HistoryEvent]) -> Vec<(f64, f64)> {
    let mut intervals = Vec::new();
    let mut started_at = None;
    for event in events {
        match event.kind {
            HistoryEventKind::ActivityStart => started_at = Some(event.timestamp),
            HistoryEventKind::ActivityEnd => {
                if let Some(start) = started_at.take()
                    && event.timestamp > start
                {
                    intervals.push((start, event.timestamp));
                }
            }
            HistoryEventKind::Request | HistoryEventKind::Activity => {}
        }
    }
    intervals
}

fn conversational_activity_intervals(events: &[&HistoryEvent]) -> Vec<(f64, f64)> {
    let mut intervals = Vec::new();
    let mut activity_start = None;
    let mut activity_end = None;
    for event in events {
        match event.kind {
            HistoryEventKind::Request => {
                push_activity_interval(&mut intervals, activity_start, activity_end);
                activity_start = Some(event.timestamp);
                activity_end = Some(event.timestamp);
            }
            HistoryEventKind::Activity => {
                if activity_start.is_some() {
                    activity_end = Some(event.timestamp);
                }
            }
            HistoryEventKind::ActivityStart | HistoryEventKind::ActivityEnd => {}
        }
    }
    push_activity_interval(&mut intervals, activity_start, activity_end);
    intervals
}

fn push_activity_interval(
    intervals: &mut Vec<(f64, f64)>,
    start: Option<f64>,
    end: Option<f64>,
) {
    if let (Some(start), Some(end)) = (start, end)
        && end > start
    {
        intervals.push((start, end));
    }
}

fn summarize_activity_intervals(intervals: Vec<(f64, f64)>) -> ActiveDurationSummary {
    let mut summary = ActiveDurationSummary {
        intervals: intervals.clone(),
        ..Default::default()
    };
    for (start, end) in intervals {
        let mut cursor = start.round() as i64;
        let end = end.round() as i64;
        while cursor < end {
            let bucket_start = half_hour_bucket_start(cursor as f64).round() as i64;
            let segment_end = end.min(bucket_start.saturating_add(30 * 60));
            let seconds = segment_end.saturating_sub(cursor);
            if seconds <= 0 {
                break;
            }
            summary.total_seconds = summary.total_seconds.saturating_add(seconds);
            *summary.seconds_by_bucket.entry(bucket_start).or_default() += seconds;
            cursor = segment_end;
        }
    }
    summary
}

pub fn history_key(source: &str, session_id: &str) -> String {
    format!("{source}:{session_id}")
}

pub fn deterministic_uuid(value: &str) -> String {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, value.as_bytes()).to_string()
}

fn min_nonzero(left: f64, right: f64) -> f64 {
    if left <= 0.0 { right } else { left.min(right) }
}

pub fn half_hour_bucket_start(timestamp: f64) -> f64 {
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

pub fn now_seconds() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or(0.0)
}
