use crate::ai_usage_store::AIUsageStore;
use crate::paths::home_dir;
use anyhow::Result;
use chrono::{DateTime, Datelike, Local, TimeZone, Timelike};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use uuid::Uuid;

const RECENT_HISTORY_SESSION_LIMIT: usize = 80;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AIHistoryProjectRequest {
    pub id: String,
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIHistorySnapshot {
    pub project_id: String,
    pub project_name: String,
    pub project_summary: AIProjectUsageSummary,
    pub sessions: Vec<AISessionSummary>,
    pub heatmap: Vec<AIHeatmapDay>,
    pub today_time_buckets: Vec<AITimeBucket>,
    pub tool_breakdown: Vec<AIUsageBreakdownItem>,
    pub model_breakdown: Vec<AIUsageBreakdownItem>,
    pub indexed_at: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIGlobalHistorySnapshot {
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub today_total_tokens: i64,
    pub today_cached_input_tokens: i64,
    pub sessions: Vec<AISessionSummary>,
    pub project_count: usize,
    pub indexed_at: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIProjectUsageSummary {
    pub project_id: String,
    pub project_name: String,
    pub current_session_tokens: i64,
    pub current_session_cached_input_tokens: i64,
    pub project_total_tokens: i64,
    pub project_cached_input_tokens: i64,
    pub today_total_tokens: i64,
    pub today_cached_input_tokens: i64,
    pub current_tool: Option<String>,
    pub current_model: Option<String>,
    pub current_session_updated_at: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AISessionSummary {
    pub session_id: String,
    pub external_session_id: Option<String>,
    pub project_id: String,
    pub project_name: String,
    pub project_path: String,
    pub session_title: String,
    pub first_seen_at: f64,
    pub last_seen_at: f64,
    pub last_tool: Option<String>,
    pub last_model: Option<String>,
    pub request_count: i64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub active_duration_seconds: i64,
    pub today_tokens: i64,
    pub today_cached_input_tokens: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIHeatmapDay {
    pub day: f64,
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub request_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AITimeBucket {
    pub start: f64,
    pub end: f64,
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub request_count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIUsageBreakdownItem {
    pub key: String,
    pub total_tokens: i64,
    pub cached_input_tokens: i64,
    pub request_count: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct HistoryEntry {
    pub(crate) source: String,
    pub(crate) session_id: String,
    pub(crate) external_session_id: Option<String>,
    pub(crate) session_title: Option<String>,
    pub(crate) timestamp: f64,
    pub(crate) model: Option<String>,
    pub(crate) input_tokens: i64,
    pub(crate) output_tokens: i64,
    pub(crate) cached_input_tokens: i64,
    pub(crate) reasoning_output_tokens: i64,
}

impl HistoryEntry {
    pub(crate) fn total_tokens(&self) -> i64 {
        self.input_tokens + self.output_tokens + self.reasoning_output_tokens
    }
}

#[derive(Debug, Clone)]
pub(crate) struct HistoryEvent {
    pub(crate) source: String,
    pub(crate) session_id: String,
    pub(crate) timestamp: f64,
    pub(crate) role: HistoryRole,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HistoryRole {
    User,
    Assistant,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct ParsedHistory {
    pub(crate) entries: Vec<HistoryEntry>,
    pub(crate) events: Vec<HistoryEvent>,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct JSONLParseSnapshot {
    pub(crate) result: ParsedHistory,
    pub(crate) last_processed_offset: i64,
    pub(crate) payload_json: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AIExternalFileCheckpointPayload {
    pub(crate) session_key: Option<String>,
    pub(crate) external_session_id: Option<String>,
    pub(crate) session_title: Option<String>,
    pub(crate) last_model: Option<String>,
    #[serde(default)]
    pub(crate) model_total_tokens_by_name: HashMap<String, i64>,
}

#[derive(Debug, Default)]
struct SessionAccumulator {
    source: String,
    session_id: String,
    external_session_id: Option<String>,
    title: Option<String>,
    first_seen_at: f64,
    last_seen_at: f64,
    model: Option<String>,
    input_tokens: i64,
    output_tokens: i64,
    cached_input_tokens: i64,
    reasoning_output_tokens: i64,
    request_count: i64,
    today_tokens: i64,
    today_cached_input_tokens: i64,
    active_duration_seconds: i64,
}

pub fn index_project_history_fresh_with_progress<F>(
    project: AIHistoryProjectRequest,
    mut on_progress: F,
) -> AIHistorySnapshot
where
    F: FnMut(f64, &'static str),
{
    load_project_history_with_home(project, &home_dir(), &mut on_progress)
}

pub fn load_indexed_project_history(
    project: AIHistoryProjectRequest,
) -> Result<Option<AIHistorySnapshot>> {
    let store = AIUsageStore::default();
    let conn = store.connect()?;
    store.indexed_project_snapshot(&conn, project)
}

pub fn rename_indexed_history_session(
    project: AIHistoryProjectRequest,
    session_id: String,
    title: String,
) -> Result<Option<AIHistorySnapshot>> {
    let store = AIUsageStore::default();
    let conn = store.connect()?;
    if !store.rename_project_session(&conn, &project.path, &session_id, &title)? {
        return Ok(None);
    }
    store.indexed_project_snapshot(&conn, project)
}

pub fn remove_indexed_history_session(
    project: AIHistoryProjectRequest,
    session_id: String,
) -> Result<Option<AIHistorySnapshot>> {
    let store = AIUsageStore::default();
    let conn = store.connect()?;
    if !store.remove_project_session(&conn, &project.path, &session_id)? {
        return Ok(None);
    }
    store.indexed_project_snapshot(&conn, project)
}

pub fn index_global_history_fresh(
    projects: Vec<AIHistoryProjectRequest>,
) -> AIGlobalHistorySnapshot {
    let home = home_dir();
    let mut total_tokens = 0;
    let mut cached_input_tokens = 0;
    let mut today_total_tokens = 0;
    let mut today_cached_input_tokens = 0;
    let mut project_count = 0;

    for project in projects {
        if project.path.trim().is_empty() {
            continue;
        }
        let snapshot = load_project_history_with_home(project, &home, &mut |_, _| {});
        total_tokens += snapshot.project_summary.project_total_tokens;
        cached_input_tokens += snapshot.project_summary.project_cached_input_tokens;
        today_total_tokens += snapshot.project_summary.today_total_tokens;
        today_cached_input_tokens += snapshot.project_summary.today_cached_input_tokens;
        project_count += 1;
    }

    AIGlobalHistorySnapshot {
        total_tokens,
        cached_input_tokens,
        today_total_tokens,
        today_cached_input_tokens,
        sessions: Vec::new(),
        project_count,
        indexed_at: now_seconds(),
    }
}

pub fn load_indexed_global_history(
    projects: Vec<AIHistoryProjectRequest>,
) -> Result<Option<AIGlobalHistorySnapshot>> {
    let store = AIUsageStore::default();
    let conn = store.connect()?;
    let now = now_seconds();
    let mut total_tokens = 0;
    let mut cached_input_tokens = 0;
    let mut today_total_tokens = 0;
    let mut today_cached_input_tokens = 0;
    let mut indexed_count = 0;
    let requested_count = projects
        .iter()
        .filter(|project| !project.path.trim().is_empty())
        .count();

    for project in projects {
        if project.path.trim().is_empty() {
            continue;
        }
        let Some(snapshot) = store.indexed_project_snapshot(&conn, project)? else {
            continue;
        };
        total_tokens += snapshot.project_summary.project_total_tokens;
        cached_input_tokens += snapshot.project_summary.project_cached_input_tokens;
        today_total_tokens += snapshot.project_summary.today_total_tokens;
        today_cached_input_tokens += snapshot.project_summary.today_cached_input_tokens;
        indexed_count += 1;
    }

    if requested_count > 0 && indexed_count == 0 {
        return Ok(None);
    }
    Ok(Some(AIGlobalHistorySnapshot {
        total_tokens,
        cached_input_tokens,
        today_total_tokens,
        today_cached_input_tokens,
        sessions: Vec::new(),
        project_count: indexed_count,
        indexed_at: now,
    }))
}

pub fn global_today_normalized_tokens() -> Result<i64> {
    let store = AIUsageStore::default();
    let conn = store.connect()?;
    store.global_today_normalized_tokens(&conn)
}

pub fn global_all_time_normalized_tokens() -> Result<i64> {
    let store = AIUsageStore::default();
    let conn = store.connect()?;
    store.global_all_time_normalized_tokens(&conn)
}

pub fn indexed_sessions_since(cutoff: Option<f64>) -> Result<Vec<AISessionSummary>> {
    let store = AIUsageStore::default();
    let conn = store.connect()?;
    store.indexed_sessions_since(&conn, cutoff)
}

pub fn normalized_project_totals_since(
    cutoff: Option<f64>,
) -> Result<Vec<crate::ai_usage_store::AIUsageProjectTotal>> {
    let store = AIUsageStore::default();
    let conn = store.connect()?;
    store.normalized_project_totals_since(&conn, cutoff)
}

fn load_project_history_with_home(
    project: AIHistoryProjectRequest,
    home: &Path,
    on_progress: &mut impl FnMut(f64, &'static str),
) -> AIHistorySnapshot {
    if project.path.trim().is_empty() {
        return build_snapshot(project, ParsedHistory::default());
    }

    on_progress(0.12, "readingSources");
    if let Ok(snapshot) = load_project_history_with_store(
        project.clone(),
        home,
        &AIUsageStore::default(),
        on_progress,
    ) {
        return snapshot;
    }

    load_project_history_without_store(project, home, on_progress)
}

fn load_project_history_without_store(
    project: AIHistoryProjectRequest,
    home: &Path,
    on_progress: &mut impl FnMut(f64, &'static str),
) -> AIHistorySnapshot {
    let mut parsed = ParsedHistory::default();
    parsed.merge(parse_claude_history(&project, home));
    on_progress(0.38, "readingSources");
    parsed.merge(parse_codex_history(&project, home));
    on_progress(0.58, "readingSources");
    parsed.merge(parse_gemini_history(&project, home));
    on_progress(0.74, "readingSources");
    parsed.merge(parse_kiro_history(&project, home));
    on_progress(0.82, "readingSources");
    parsed.merge(parse_opencode_history(&project, home));
    on_progress(0.88, "readingSources");
    on_progress(0.96, "aggregating");
    build_snapshot(project, parsed)
}

fn load_project_history_with_store(
    project: AIHistoryProjectRequest,
    home: &Path,
    store: &AIUsageStore,
    on_progress: &mut impl FnMut(f64, &'static str),
) -> Result<AIHistorySnapshot> {
    if project.path.trim().is_empty() {
        return Ok(build_snapshot(project, ParsedHistory::default()));
    }

    let conn = store.connect()?;
    for file_path in claude_project_log_paths(&project.path, home) {
        let _ = store.load_or_index_jsonl_file(
            &conn,
            "claude",
            &file_path,
            &project,
            |checkpoint| {
                let seed = checkpoint.and_then(|checkpoint| {
                    decode_checkpoint_payload(checkpoint.payload_json.as_deref())
                });
                parse_claude_history_file_snapshot(
                    &project,
                    &file_path,
                    checkpoint.map(|item| item.last_offset).unwrap_or(0),
                    seed.as_ref(),
                )
            },
            || parse_claude_history_file_snapshot(&project, &file_path, 0, None),
        )?;
    }
    on_progress(0.38, "readingSources");
    for file_path in codex_session_paths(&project.path, home) {
        let _ = store.load_or_index_jsonl_file(
            &conn,
            "codex",
            &file_path,
            &project,
            |checkpoint| {
                let seed = checkpoint.and_then(|checkpoint| {
                    decode_checkpoint_payload(checkpoint.payload_json.as_deref())
                });
                parse_codex_history_file_snapshot(
                    &project,
                    &file_path,
                    checkpoint.map(|item| item.last_offset).unwrap_or(0),
                    seed.as_ref(),
                )
            },
            || parse_codex_history_file_snapshot(&project, &file_path, 0, None),
        )?;
    }
    on_progress(0.58, "readingSources");
    for file_path in gemini_session_paths(&project.path, home) {
        let _ = store.load_or_index_file(&conn, "gemini", &file_path, &project, || {
            parse_gemini_history_file(&project, &file_path)
        })?;
    }
    on_progress(0.74, "readingSources");
    for file_path in kiro_session_paths(&project.path, home) {
        let _ = store.load_or_index_file(&conn, "kiro", &file_path, &project, || {
            parse_kiro_history_file(&project, &file_path)
        })?;
    }
    on_progress(0.82, "readingSources");
    for file_path in opencode_history_source_paths(home) {
        let source = if file_path.extension().and_then(|value| value.to_str()) == Some("db") {
            "opencode"
        } else {
            "opencode"
        };
        let _ = store.load_or_index_file(&conn, source, &file_path, &project, || {
            parse_opencode_history_file(&project, &file_path)
        })?;
    }
    on_progress(0.88, "readingSources");
    on_progress(0.96, "aggregating");
    let project_path = project.path.clone();
    let snapshot = store.project_snapshot(&conn, project)?;
    store.save_project_index_state(&conn, &snapshot, &project_path)?;
    Ok(snapshot)
}

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

fn parse_claude_history(project: &AIHistoryProjectRequest, home: &Path) -> ParsedHistory {
    let mut result = ParsedHistory::default();
    for file_path in claude_project_log_paths(&project.path, home) {
        result.merge(parse_claude_history_file(project, &file_path));
    }
    result
}

fn parse_claude_history_file(project: &AIHistoryProjectRequest, file_path: &Path) -> ParsedHistory {
    parse_claude_history_file_snapshot(project, file_path, 0, None).result
}

fn parse_claude_history_file_snapshot(
    project: &AIHistoryProjectRequest,
    file_path: &Path,
    starting_at: i64,
    seed: Option<&AIExternalFileCheckpointPayload>,
) -> JSONLParseSnapshot {
    let mut result = ParsedHistory::default();
    let mut last_processed_offset = starting_at.max(0);
    let mut cwd_confirmed = false;
    let mut cwd_denied = false;
    let mut early_line_count = 0;
    let mut seen_assistant_ids = HashMap::<String, bool>::new();
    let mut payload = seed.cloned().unwrap_or_default();
    if starting_at > 0 || payload.session_key.is_some() {
        cwd_confirmed = true;
    }

    let stop_on_invalid_json = starting_at > 0;
    let _ = for_each_jsonl_line(file_path, starting_at, |line, end_offset| {
        if cwd_denied {
            return false;
        }
        let Ok(row) = serde_json::from_str::<Value>(&line) else {
            return !stop_on_invalid_json;
        };
        if !cwd_confirmed {
            if let Some(cwd) = row.get("cwd").and_then(|value| value.as_str()) {
                if paths_equivalent(Some(cwd), &project.path) {
                    cwd_confirmed = true;
                } else {
                    cwd_denied = true;
                    return false;
                }
            }
        }
        if !cwd_confirmed {
            last_processed_offset = end_offset;
            early_line_count += 1;
            return early_line_count < 10;
        }
        let Some(session_id) = row
            .get("sessionId")
            .and_then(|value| value.as_str())
            .and_then(normalized_string)
        else {
            last_processed_offset = end_offset;
            return true;
        };
        let timestamp = row
            .get("timestamp")
            .and_then(|value| value.as_str())
            .and_then(parse_iso8601_seconds)
            .unwrap_or_else(now_seconds);
        let row_type = row.get("type").and_then(|value| value.as_str());
        if let Some(role) = claude_role(row_type) {
            result.events.push(HistoryEvent {
                source: "claude".to_string(),
                session_id: session_id.clone(),
                timestamp,
                role,
            });
            payload.session_key = Some(session_id.clone());
            payload.external_session_id = Some(session_id.clone());
            payload.session_title = claude_title(&row)
                .or(payload.session_title.clone())
                .or_else(|| Some(project.name.clone()));
        }
        if row_type != Some("assistant") {
            last_processed_offset = end_offset;
            return true;
        }
        if let Some(uuid) = row
            .get("uuid")
            .and_then(|value| value.as_str())
            .and_then(normalized_string)
        {
            if seen_assistant_ids.insert(uuid, true).is_some() {
                last_processed_offset = end_offset;
                return true;
            }
        }
        let message = row.get("message").unwrap_or(&Value::Null);
        let usage = message.get("usage").unwrap_or(&Value::Null);
        let input_tokens = json_i64(usage.get("input_tokens"));
        let output_tokens = json_i64(usage.get("output_tokens"));
        let cached_input_tokens = json_i64(usage.get("cache_read_input_tokens"));
        let total_tokens = input_tokens + output_tokens + cached_input_tokens;
        if total_tokens <= 0 {
            last_processed_offset = end_offset;
            return true;
        }
        let model = message
            .get("model")
            .and_then(|value| value.as_str())
            .and_then(normalized_string)
            .or_else(|| Some("unknown".to_string()));
        payload.last_model = model.clone().or(payload.last_model.clone());
        result.entries.push(HistoryEntry {
            source: "claude".to_string(),
            session_id: session_id.clone(),
            external_session_id: Some(session_id),
            session_title: claude_title(&row).or_else(|| Some(project.name.clone())),
            timestamp,
            model,
            input_tokens,
            output_tokens,
            cached_input_tokens,
            reasoning_output_tokens: 0,
        });
        last_processed_offset = end_offset;
        true
    });

    JSONLParseSnapshot {
        result: if cwd_denied {
            ParsedHistory::default()
        } else {
            result
        },
        last_processed_offset,
        payload_json: encode_checkpoint_payload(&payload),
    }
}

fn parse_codex_history(project: &AIHistoryProjectRequest, home: &Path) -> ParsedHistory {
    let mut result = ParsedHistory::default();
    for file_path in codex_session_paths(&project.path, home) {
        result.merge(parse_codex_history_file(project, &file_path));
    }
    result
}

fn parse_codex_history_file(project: &AIHistoryProjectRequest, file_path: &Path) -> ParsedHistory {
    parse_codex_history_file_snapshot(project, file_path, 0, None).result
}

fn parse_codex_history_file_snapshot(
    project: &AIHistoryProjectRequest,
    file_path: &Path,
    starting_at: i64,
    seed: Option<&AIExternalFileCheckpointPayload>,
) -> JSONLParseSnapshot {
    let mut result = ParsedHistory::default();
    let mut payload = seed.cloned().unwrap_or_default();
    let mut matched_project = payload.session_key.is_some();
    let mut session_id = payload
        .session_key
        .clone()
        .unwrap_or_else(|| file_path.display().to_string());
    let mut session_title: Option<String> = payload.session_title.clone();
    let mut model: Option<String> = payload.last_model.clone();
    let mut total_by_model = payload.model_total_tokens_by_name.clone();
    let mut pending_entries = Vec::new();
    let mut pending_events = Vec::new();
    let mut last_processed_offset = starting_at.max(0);
    let stop_on_invalid_json = starting_at > 0;

    let _ = for_each_jsonl_line(file_path, starting_at, |line, end_offset| {
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            return !stop_on_invalid_json;
        };
        let Some(timestamp) = row
            .get("timestamp")
            .and_then(|value| value.as_str())
            .and_then(parse_iso8601_seconds)
        else {
            last_processed_offset = end_offset;
            return true;
        };
        let row_type = row.get("type").and_then(|value| value.as_str());
        let payload = row.get("payload").unwrap_or(&Value::Null);
        if row_type == Some("session_meta") {
            if payload
                .get("cwd")
                .and_then(|value| value.as_str())
                .map(|cwd| paths_equivalent(Some(cwd), &project.path))
                .unwrap_or(false)
            {
                matched_project = true;
                if let Some(id) = payload
                    .get("id")
                    .and_then(|value| value.as_str())
                    .and_then(normalized_string)
                {
                    session_id = id;
                }
                session_title = payload
                    .get("thread_name")
                    .and_then(|value| value.as_str())
                    .and_then(normalized_string)
                    .or_else(|| {
                        payload
                            .get("title")
                            .and_then(|value| value.as_str())
                            .and_then(normalized_string)
                    })
                    .or(session_title.clone());
            }
        }
        if row_type == Some("turn_context") {
            if payload
                .get("cwd")
                .and_then(|value| value.as_str())
                .map(|cwd| paths_equivalent(Some(cwd), &project.path))
                .unwrap_or(false)
            {
                matched_project = true;
                model = payload
                    .get("model")
                    .and_then(|value| value.as_str())
                    .and_then(normalized_string)
                    .or(model.clone());
            }
        }
        if !matched_project {
            last_processed_offset = end_offset;
            return true;
        }
        if row_type == Some("response_item") && session_title.is_none() {
            session_title = codex_response_title(payload);
        }
        pending_events.push(HistoryEvent {
            source: "codex".to_string(),
            session_id: session_id.clone(),
            timestamp,
            role: codex_role(row_type),
        });
        if row_type != Some("event_msg")
            || payload.get("type").and_then(|value| value.as_str()) != Some("token_count")
        {
            last_processed_offset = end_offset;
            return true;
        }
        let info = payload.get("info").unwrap_or(&Value::Null);
        let resolved_model = info
            .get("model")
            .and_then(|value| value.as_str())
            .and_then(normalized_string)
            .or_else(|| {
                payload
                    .get("model")
                    .and_then(|value| value.as_str())
                    .and_then(normalized_string)
            })
            .or_else(|| model.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let last_usage = codex_history_usage(info.get("last_token_usage"));
        let total_usage = codex_history_usage(info.get("total_token_usage"));
        let usage = if let Some(total_usage) = total_usage {
            let previous = *total_by_model.get(&resolved_model).unwrap_or(&0);
            let current = total_usage.total_tokens();
            let delta = (current - previous).max(0);
            total_by_model.insert(resolved_model.clone(), previous.max(current));
            if delta <= 0 {
                None
            } else if last_usage.as_ref().map(|usage| usage.total_tokens()) == Some(delta) {
                last_usage
            } else {
                Some(total_usage.delta(delta))
            }
        } else {
            last_usage
        };
        let Some(usage) = usage else {
            last_processed_offset = end_offset;
            return true;
        };
        if usage.total_tokens() <= 0 && usage.cached_input_tokens <= 0 {
            last_processed_offset = end_offset;
            return true;
        }
        pending_entries.push(HistoryEntry {
            source: "codex".to_string(),
            session_id: session_id.clone(),
            external_session_id: Some(session_id.clone()),
            session_title: session_title.clone().or_else(|| Some(project.name.clone())),
            timestamp,
            model: Some(resolved_model.clone()),
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cached_input_tokens: usage.cached_input_tokens,
            reasoning_output_tokens: usage.reasoning_output_tokens,
        });
        model = Some(resolved_model);
        last_processed_offset = end_offset;
        true
    });
    if matched_project {
        result.events.extend(pending_events);
        result.entries.extend(pending_entries);
    }
    payload.session_key = matched_project
        .then(|| session_id.clone())
        .or(payload.session_key.clone());
    payload.external_session_id = matched_project
        .then(|| session_id.clone())
        .or(payload.external_session_id.clone());
    payload.session_title = session_title.or(payload.session_title.clone());
    payload.last_model = model.or(payload.last_model.clone());
    payload.model_total_tokens_by_name = total_by_model;

    JSONLParseSnapshot {
        result,
        last_processed_offset,
        payload_json: encode_checkpoint_payload(&payload),
    }
}

fn parse_gemini_history(project: &AIHistoryProjectRequest, home: &Path) -> ParsedHistory {
    let mut result = ParsedHistory::default();
    for file_path in gemini_session_paths(&project.path, home) {
        result.merge(parse_gemini_history_file(project, &file_path));
    }
    result
}

fn parse_kiro_history(project: &AIHistoryProjectRequest, home: &Path) -> ParsedHistory {
    let mut result = ParsedHistory::default();
    for file_path in kiro_session_paths(&project.path, home) {
        result.merge(parse_kiro_history_file(project, &file_path));
    }
    result
}

fn parse_kiro_history_file(project: &AIHistoryProjectRequest, file_path: &Path) -> ParsedHistory {
    let Ok(data) = fs::read_to_string(file_path) else {
        return ParsedHistory::default();
    };
    let Ok(value) = serde_json::from_str::<Value>(&data) else {
        return ParsedHistory::default();
    };
    let Some(session_id) = kiro_session_id(&value, file_path) else {
        return ParsedHistory::default();
    };
    let Some(project_path) = kiro_project_path(&value) else {
        return ParsedHistory::default();
    };
    if !paths_equivalent(Some(&project_path), &project.path) {
        return ParsedHistory::default();
    }

    let model = kiro_model(&value);
    let session_title = kiro_session_title(&value).or_else(|| Some(project.name.clone()));
    let timestamps = kiro_history_timestamps(&value);
    let last_timestamp = timestamps.last().copied().unwrap_or_else(now_seconds);
    let mut result = ParsedHistory::default();
    let mut last_role = None;
    for timestamp in &timestamps {
        let role = if last_role == Some(HistoryRole::User) {
            HistoryRole::Assistant
        } else {
            HistoryRole::User
        };
        last_role = Some(role);
        result.events.push(HistoryEvent {
            source: "kiro".to_string(),
            session_id: session_id.clone(),
            timestamp: *timestamp,
            role,
        });
    }

    let usage = kiro_usage(&value);
    if usage.total_tokens() > 0 || usage.cached_input_tokens > 0 {
        result.entries.push(HistoryEntry {
            source: "kiro".to_string(),
            session_id: session_id.clone(),
            external_session_id: Some(session_id.clone()),
            session_title,
            timestamp: last_timestamp,
            model,
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cached_input_tokens: usage.cached_input_tokens,
            reasoning_output_tokens: usage.reasoning_output_tokens,
        });
    }
    result
}

fn parse_gemini_history_file(project: &AIHistoryProjectRequest, file_path: &Path) -> ParsedHistory {
    let mut result = ParsedHistory::default();
    let Ok(data) = fs::read(file_path) else {
        return result;
    };
    let Ok(object) = serde_json::from_slice::<Value>(&data) else {
        return result;
    };
    let Some(session_id) = object
        .get("sessionId")
        .and_then(|value| value.as_str())
        .and_then(normalized_string)
    else {
        return result;
    };
    let messages = object
        .get("messages")
        .or_else(|| object.get("history"))
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();
    let mut session_title = None;
    let mut session_model = object
        .get("model")
        .and_then(|value| value.as_str())
        .and_then(normalized_string);
    for message in messages {
        let timestamp = message
            .get("timestamp")
            .or_else(|| message.get("createTime"))
            .or_else(|| object.get("createTime"))
            .and_then(|value| value.as_str())
            .and_then(parse_iso8601_seconds)
            .unwrap_or_else(now_seconds);
        let message_type = message
            .get("type")
            .or_else(|| message.get("role"))
            .and_then(|value| value.as_str());
        let role = if message_type == Some("user") {
            HistoryRole::User
        } else {
            HistoryRole::Assistant
        };
        result.events.push(HistoryEvent {
            source: "gemini".to_string(),
            session_id: session_id.clone(),
            timestamp,
            role,
        });
        if role == HistoryRole::User && session_title.is_none() {
            session_title = parse_gemini_title(message.get("content"));
        }
        let resolved_model = message
            .get("model")
            .and_then(|value| value.as_str())
            .and_then(normalized_string)
            .or_else(|| session_model.clone())
            .unwrap_or_else(|| "unknown".to_string());
        session_model = Some(resolved_model.clone());
        let usage = message
            .get("tokens")
            .map(gemini_tokens_usage)
            .or_else(|| message.get("usage").map(gemini_usage_metadata))
            .or_else(|| message.get("usageMetadata").map(gemini_usage_metadata))
            .or_else(|| message.get("token_count").map(gemini_usage_metadata));
        let Some(usage) = usage else {
            continue;
        };
        if usage.total_tokens() <= 0 && usage.cached_input_tokens <= 0 {
            continue;
        }
        result.entries.push(HistoryEntry {
            source: "gemini".to_string(),
            session_id: session_id.clone(),
            external_session_id: Some(session_id.clone()),
            session_title: session_title.clone().or_else(|| Some(project.name.clone())),
            timestamp,
            model: Some(resolved_model),
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cached_input_tokens: usage.cached_input_tokens,
            reasoning_output_tokens: usage.reasoning_output_tokens,
        });
    }
    result
}

fn parse_opencode_history(project: &AIHistoryProjectRequest, home: &Path) -> ParsedHistory {
    let mut result = ParsedHistory::default();
    for file_path in opencode_history_source_paths(home) {
        result.merge(parse_opencode_history_file(project, &file_path));
    }
    result
}

fn parse_opencode_history_file(
    project: &AIHistoryProjectRequest,
    file_path: &Path,
) -> ParsedHistory {
    if file_path.extension().and_then(|value| value.to_str()) == Some("db") {
        parse_opencode_database(project, file_path)
    } else {
        parse_opencode_legacy_message_file(project, file_path)
    }
}

fn parse_opencode_database(project: &AIHistoryProjectRequest, file_path: &Path) -> ParsedHistory {
    let mut result = ParsedHistory::default();
    let Ok(conn) = Connection::open(file_path) else {
        return result;
    };
    let Ok(mut statement) = conn.prepare(
        r#"
        SELECT s.id, s.title, m.data
        FROM session s
        JOIN message m ON m.session_id = s.id
        WHERE s.time_archived IS NULL
        ORDER BY m.time_created ASC;
        "#,
    ) else {
        return result;
    };
    let Ok(rows) = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, String>(2)?,
        ))
    }) else {
        return result;
    };

    for row in rows.flatten() {
        let (session_id, title, data) = row;
        let Ok(payload) = serde_json::from_str::<Value>(&data) else {
            continue;
        };
        let Some(root_path) = payload
            .get("path")
            .and_then(|value| value.get("root"))
            .and_then(|value| value.as_str())
        else {
            continue;
        };
        if !paths_equivalent(Some(root_path), &project.path) {
            continue;
        }
        let Some(timestamp) = payload
            .get("time")
            .and_then(|value| value.get("created"))
            .and_then(value_to_string)
            .and_then(|value| parse_opencode_timestamp(&value))
        else {
            continue;
        };
        let role = if payload.get("role").and_then(|value| value.as_str()) == Some("user") {
            HistoryRole::User
        } else {
            HistoryRole::Assistant
        };
        result.events.push(HistoryEvent {
            source: "opencode".to_string(),
            session_id: session_id.clone(),
            timestamp,
            role,
        });
        let model = payload
            .get("modelID")
            .and_then(|value| value.as_str())
            .and_then(normalized_string)
            .unwrap_or_else(|| "unknown".to_string());
        let usage = opencode_tokens_usage(payload.get("tokens").unwrap_or(&Value::Null));
        if usage.total_tokens() <= 0 && usage.cached_input_tokens <= 0 {
            continue;
        }
        result.entries.push(HistoryEntry {
            source: "opencode".to_string(),
            session_id: session_id.clone(),
            external_session_id: Some(session_id.clone()),
            session_title: title
                .as_deref()
                .and_then(normalized_string)
                .or_else(|| Some(project.name.clone())),
            timestamp,
            model: Some(model),
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cached_input_tokens: usage.cached_input_tokens,
            reasoning_output_tokens: usage.reasoning_output_tokens,
        });
    }
    result
}

fn parse_opencode_legacy_message_file(
    project: &AIHistoryProjectRequest,
    file_path: &Path,
) -> ParsedHistory {
    let mut result = ParsedHistory::default();
    let Ok(data) = fs::read(file_path) else {
        return result;
    };
    let Ok(payload) = serde_json::from_slice::<Value>(&data) else {
        return result;
    };
    let Some(root_path) = payload
        .get("path")
        .and_then(|value| value.get("root"))
        .and_then(|value| value.as_str())
    else {
        return result;
    };
    if !paths_equivalent(Some(root_path), &project.path) {
        return result;
    }
    let Some(timestamp) = payload
        .get("time")
        .and_then(|value| value.get("created"))
        .and_then(value_to_string)
        .and_then(|value| parse_opencode_timestamp(&value))
    else {
        return result;
    };
    let session_id = file_path
        .parent()
        .and_then(|path| path.file_name())
        .and_then(|value| value.to_str())
        .and_then(normalized_string)
        .unwrap_or_else(|| file_path.display().to_string());
    let role = if payload.get("role").and_then(|value| value.as_str()) == Some("user") {
        HistoryRole::User
    } else {
        HistoryRole::Assistant
    };
    result.events.push(HistoryEvent {
        source: "opencode".to_string(),
        session_id: session_id.clone(),
        timestamp,
        role,
    });
    let model = payload
        .get("modelID")
        .and_then(|value| value.as_str())
        .and_then(normalized_string)
        .unwrap_or_else(|| "unknown".to_string());
    let usage = opencode_tokens_usage(payload.get("tokens").unwrap_or(&Value::Null));
    if usage.total_tokens() > 0 || usage.cached_input_tokens > 0 {
        result.entries.push(HistoryEntry {
            source: "opencode".to_string(),
            session_id: session_id.clone(),
            external_session_id: Some(session_id),
            session_title: Some(project.name.clone()),
            timestamp,
            model: Some(model),
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cached_input_tokens: usage.cached_input_tokens,
            reasoning_output_tokens: usage.reasoning_output_tokens,
        });
    }
    result
}

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
    if left <= 0.0 {
        right
    } else {
        left.min(right)
    }
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

pub(crate) fn local_day_start_seconds(timestamp: f64) -> f64 {
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

fn parse_iso8601_seconds(value: &str) -> Option<f64> {
    DateTime::parse_from_rfc3339(value).ok().map(|date| {
        date.timestamp() as f64 + f64::from(date.timestamp_subsec_micros()) / 1_000_000.0
    })
}

fn parse_opencode_timestamp(value: &str) -> Option<f64> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(milliseconds) = value.parse::<f64>() {
        return Some(milliseconds / 1000.0);
    }
    parse_iso8601_seconds(value)
}

fn value_to_string(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_string)
        .or_else(|| value.as_i64().map(|value| value.to_string()))
        .or_else(|| value.as_f64().map(|value| value.to_string()))
}

fn claude_role(row_type: Option<&str>) -> Option<HistoryRole> {
    match row_type {
        Some("user") => Some(HistoryRole::User),
        Some("assistant") | Some("tool_use") | Some("tool_result") => Some(HistoryRole::Assistant),
        _ => None,
    }
}

fn codex_role(row_type: Option<&str>) -> HistoryRole {
    if matches!(row_type, Some("turn_context") | Some("session_meta")) {
        HistoryRole::User
    } else {
        HistoryRole::Assistant
    }
}

fn decode_checkpoint_payload(value: Option<&str>) -> Option<AIExternalFileCheckpointPayload> {
    value.and_then(|value| serde_json::from_str(value).ok())
}

fn encode_checkpoint_payload(payload: &AIExternalFileCheckpointPayload) -> Option<String> {
    serde_json::to_string(payload).ok()
}

fn claude_title(row: &Value) -> Option<String> {
    if row.get("type").and_then(|value| value.as_str()) != Some("user") {
        return row
            .get("slug")
            .and_then(|value| value.as_str())
            .and_then(normalized_string);
    }
    let message = row.get("message").unwrap_or(&Value::Null);
    if let Some(content) = message
        .get("content")
        .and_then(|value| value.as_str())
        .and_then(normalized_string)
    {
        return Some(truncate_title(&content));
    }
    if let Some(items) = message.get("content").and_then(|value| value.as_array()) {
        for item in items {
            if let Some(text) = item
                .get("text")
                .and_then(|value| value.as_str())
                .and_then(normalized_string)
            {
                return Some(truncate_title(&text));
            }
        }
    }
    row.get("slug")
        .and_then(|value| value.as_str())
        .and_then(normalized_string)
}

fn codex_response_title(payload: &Value) -> Option<String> {
    if payload.get("type").and_then(|value| value.as_str()) != Some("message")
        || payload.get("role").and_then(|value| value.as_str()) != Some("user")
    {
        return None;
    }
    let content = payload.get("content").and_then(|value| value.as_array())?;
    for item in content {
        let Some(text) = item
            .get("text")
            .and_then(|value| value.as_str())
            .and_then(normalized_string)
        else {
            continue;
        };
        if !text.contains("<environment_context>") {
            return Some(truncate_title(&text));
        }
    }
    None
}

fn parse_gemini_title(content: Option<&Value>) -> Option<String> {
    match content? {
        Value::String(text) => Some(truncate_title(text)),
        Value::Array(items) => items.iter().find_map(|item| {
            item.get("text")
                .and_then(|value| value.as_str())
                .and_then(normalized_string)
                .map(|text| truncate_title(&text))
                .or_else(|| parse_gemini_title(item.get("content")))
        }),
        _ => None,
    }
}

fn truncate_title(value: &str) -> String {
    value
        .replace('\n', " ")
        .chars()
        .take(80)
        .collect::<String>()
        .trim()
        .to_string()
}

fn claude_project_log_paths(project_path: &str, home: &Path) -> Vec<PathBuf> {
    let directory_name = project_path.replace('/', "-").replace('.', "-");
    directory_files(
        &home.join(".claude").join("projects").join(directory_name),
        "jsonl",
    )
}

fn gemini_session_paths(project_path: &str, home: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for root_dir in gemini_data_roots(home) {
        let temp_dir = root_dir.join("tmp");
        let projects_path = root_dir.join("projects.json");
        if let Ok(data) = fs::read(projects_path) {
            if let Ok(root) = serde_json::from_slice::<Value>(&data) {
                if let Some(projects) = root.get("projects").and_then(|value| value.as_object()) {
                    for (stored_path, value) in projects {
                        if paths_equivalent(Some(stored_path), project_path) {
                            if let Some(directory) = value.as_str().and_then(normalized_string) {
                                dirs.push(temp_dir.join(directory));
                            }
                        }
                    }
                }
            }
        }
        if let Ok(entries) = fs::read_dir(&temp_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let marker = path.join(".project_root");
                if let Ok(value) = fs::read_to_string(marker) {
                    if paths_equivalent(Some(value.trim()), project_path) {
                        dirs.push(path);
                    }
                }
            }
        }
    }
    let mut files = Vec::new();
    for dir in dirs {
        files.extend(directory_files(&dir.join("chats"), "json"));
    }
    files.retain(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.starts_with("session-"))
            .unwrap_or(false)
    });
    files.sort_by_key(|path| std::cmp::Reverse(file_modified_millis(path).unwrap_or(0)));
    files
}

fn gemini_data_roots(home: &Path) -> Vec<PathBuf> {
    let gemini_root = home.join(".gemini");
    vec![gemini_root.clone(), gemini_root.join("antigravity-cli")]
}

fn codex_session_paths(project_path: &str, home: &Path) -> Vec<PathBuf> {
    let database_path = home.join(".codex").join("state_5.sqlite");
    let from_database = codex_session_paths_from_database(project_path, &database_path);
    if !from_database.is_empty() {
        return from_database;
    }
    recursive_files(&home.join(".codex").join("sessions"), "jsonl")
        .into_iter()
        .filter(|path| codex_rollout_file_belongs_to_project(path, project_path))
        .collect()
}

fn codex_session_paths_from_database(project_path: &str, database_path: &Path) -> Vec<PathBuf> {
    if !database_path.exists() {
        return Vec::new();
    }
    let Ok(conn) = Connection::open(database_path) else {
        return Vec::new();
    };
    let Ok(mut statement) = conn.prepare(
        r#"
        SELECT rollout_path, cwd
        FROM threads
        WHERE rollout_path IS NOT NULL
        ORDER BY updated_at DESC;
        "#,
    ) else {
        return Vec::new();
    };
    let Ok(rows) = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
    }) else {
        return Vec::new();
    };

    let mut files = Vec::new();
    let mut seen = HashMap::<String, bool>::new();
    for row in rows.flatten() {
        let (rollout_path, cwd) = row;
        if !paths_equivalent(cwd.as_deref(), project_path) {
            continue;
        }
        if rollout_path.trim().is_empty() || seen.insert(rollout_path.clone(), true).is_some() {
            continue;
        }
        let file_path = PathBuf::from(rollout_path);
        if file_path.exists() {
            files.push(file_path);
        }
    }
    files
}

fn codex_rollout_file_belongs_to_project(file_path: &Path, project_path: &str) -> bool {
    let mut line_count = 0usize;
    let mut matches_project = false;
    let _ = for_each_jsonl_line(file_path, 0, |line, _| {
        line_count += 1;
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            return line_count < 20;
        };
        let row_type = row.get("type").and_then(|value| value.as_str());
        let payload = row.get("payload").unwrap_or(&Value::Null);
        if matches!(row_type, Some("session_meta") | Some("turn_context")) {
            if let Some(cwd) = payload.get("cwd").and_then(|value| value.as_str()) {
                matches_project = paths_equivalent(Some(cwd), project_path);
                return false;
            }
        }
        line_count < 20
    });
    matches_project
}

fn opencode_history_source_paths(home: &Path) -> Vec<PathBuf> {
    let database_path = home
        .join(".local")
        .join("share")
        .join("opencode")
        .join("opencode.db");
    if database_path.exists() {
        return vec![database_path];
    }
    opencode_legacy_message_paths(home)
}

fn kiro_session_paths(project_path: &str, home: &Path) -> Vec<PathBuf> {
    let sessions_dir = home.join(".kiro").join("sessions").join("cli");
    let files = directory_files(&sessions_dir, "json");
    let mut matched = files
        .into_iter()
        .filter(|path| kiro_file_belongs_to_project(path, project_path))
        .collect::<Vec<_>>();
    matched.sort_by_key(|path| std::cmp::Reverse(file_modified_millis(path).unwrap_or(0)));
    matched
}

fn kiro_file_belongs_to_project(file_path: &Path, project_path: &str) -> bool {
    let Ok(data) = fs::read_to_string(file_path) else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<Value>(&data) else {
        return false;
    };
    kiro_project_path(&value)
        .map(|path| paths_equivalent(Some(&path), project_path))
        .unwrap_or(false)
}

fn kiro_session_id(value: &Value, file_path: &Path) -> Option<String> {
    value
        .get("sessionId")
        .and_then(|v| v.as_str())
        .and_then(normalized_string)
        .or_else(|| {
            value
                .get("session")
                .and_then(|v| v.get("id"))
                .and_then(|v| v.as_str())
                .and_then(normalized_string)
        })
        .or_else(|| {
            file_path
                .file_stem()
                .and_then(|name| name.to_str())
                .and_then(normalized_string)
        })
}

fn kiro_project_path(value: &Value) -> Option<String> {
    value
        .get("projectPath")
        .and_then(|v| v.as_str())
        .and_then(normalized_string)
        .or_else(|| {
            value
                .get("project")
                .and_then(|v| v.get("path"))
                .and_then(|v| v.as_str())
                .and_then(normalized_string)
        })
        .or_else(|| {
            value
                .get("cwd")
                .and_then(|v| v.as_str())
                .and_then(normalized_string)
        })
        .or_else(|| {
            value
                .get("workingDirectory")
                .and_then(|v| v.as_str())
                .and_then(normalized_string)
        })
}

fn kiro_model(value: &Value) -> Option<String> {
    value
        .get("model")
        .and_then(|v| v.as_str())
        .and_then(normalized_string)
        .or_else(|| {
            value
                .get("session")
                .and_then(|v| v.get("model"))
                .and_then(|v| v.as_str())
                .and_then(normalized_string)
        })
}

fn kiro_session_title(value: &Value) -> Option<String> {
    value
        .get("title")
        .and_then(|v| v.as_str())
        .and_then(normalized_string)
        .or_else(|| {
            value
                .get("session")
                .and_then(|v| v.get("title"))
                .and_then(|v| v.as_str())
                .and_then(normalized_string)
        })
}

fn kiro_history_timestamps(value: &Value) -> Vec<f64> {
    let mut timestamps = value
        .get("messages")
        .and_then(|v| v.as_array())
        .map(|messages| {
            messages
                .iter()
                .filter_map(|message| {
                    message
                        .get("timestamp")
                        .or_else(|| message.get("createdAt"))
                        .and_then(value_to_string)
                        .and_then(|value| parse_iso8601_seconds(&value))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if timestamps.is_empty() {
        if let Some(value) = value
            .get("updatedAt")
            .and_then(|v| v.as_f64().or_else(|| v.as_i64().map(|value| value as f64)))
        {
            timestamps.push(value);
        }
    }
    timestamps.sort_by(|left, right| left.total_cmp(right));
    timestamps
}

fn kiro_usage(value: &Value) -> HistoryUsage {
    let usage = value.get("usage").unwrap_or(&Value::Null);
    HistoryUsage {
        input_tokens: json_i64(
            usage
                .get("input")
                .or_else(|| usage.get("input_tokens"))
                .or_else(|| value.get("inputTokens")),
        ),
        output_tokens: json_i64(
            usage
                .get("output")
                .or_else(|| usage.get("output_tokens"))
                .or_else(|| value.get("outputTokens")),
        ),
        cached_input_tokens: json_i64(
            usage
                .get("cache")
                .and_then(|cache| cache.get("read"))
                .or_else(|| usage.get("cached_input_tokens"))
                .or_else(|| value.get("cachedInputTokens")),
        ),
        reasoning_output_tokens: json_i64(
            usage
                .get("reasoning")
                .or_else(|| value.get("reasoningTokens")),
        ),
    }
}

fn opencode_legacy_message_paths(home: &Path) -> Vec<PathBuf> {
    let messages_dir = home
        .join(".local")
        .join("share")
        .join("opencode")
        .join("storage")
        .join("message");
    let Ok(entries) = fs::read_dir(messages_dir) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir()
            || !dir
                .file_name()
                .and_then(|value| value.to_str())
                .map(|value| value.starts_with("ses_"))
                .unwrap_or(false)
        {
            continue;
        }
        files.extend(directory_files(&dir, "json"));
    }
    files.sort();
    files
}

fn directory_files(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some(extension))
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn recursive_files(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_recursive_files(dir, extension, &mut files);
    files.sort();
    files
}

fn collect_recursive_files(dir: &Path, extension: &str, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_recursive_files(&path, extension, files);
        } else if path.extension().and_then(|value| value.to_str()) == Some(extension) {
            files.push(path);
        }
    }
}

fn for_each_jsonl_line<F>(file_path: &Path, starting_at: i64, mut body: F) -> std::io::Result<()>
where
    F: FnMut(&str, i64) -> bool,
{
    let mut file = fs::File::open(file_path)?;
    let offset = starting_at.max(0) as u64;
    file.seek(SeekFrom::Start(offset))?;
    let mut reader = BufReader::new(file);
    let mut current_offset = offset;
    loop {
        let mut line = String::new();
        let byte_count = reader.read_line(&mut line)?;
        if byte_count == 0 {
            break;
        }
        current_offset = current_offset.saturating_add(byte_count as u64);
        let line = line.trim_end_matches(['\n', '\r']);
        if line.is_empty() {
            continue;
        }
        if !body(line, current_offset.min(i64::MAX as u64) as i64) {
            break;
        }
    }
    Ok(())
}

fn file_modified_millis(path: &Path) -> Option<u128> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis())
}

fn paths_equivalent(left: Option<&str>, right: &str) -> bool {
    let Some(left) = left.and_then(normalized_history_path) else {
        return false;
    };
    let Some(right) = normalized_history_path(right) else {
        return false;
    };
    left == right
}

fn normalized_history_path(value: &str) -> Option<String> {
    let mut value = value.trim();
    if value.is_empty() {
        return None;
    }
    value = value
        .strip_prefix(r"\\?\")
        .or_else(|| value.strip_prefix(r"//?/"))
        .unwrap_or(value);
    let mut normalized = value.replace('\\', "/");
    while normalized.ends_with('/') && !is_path_root(&normalized) {
        normalized.pop();
    }
    if has_windows_drive_prefix(&normalized) {
        normalized = normalized.to_ascii_lowercase();
    }
    Some(normalized)
}

fn has_windows_drive_prefix(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic()
}

fn is_path_root(value: &str) -> bool {
    value == "/" || (value.len() == 3 && has_windows_drive_prefix(value) && value.ends_with('/'))
}

fn normalized_string(value: &str) -> Option<String> {
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

fn json_i64(value: Option<&Value>) -> i64 {
    value
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_f64().map(|value| value as i64))
                .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregates_claude_history() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = "/tmp/project-a";
        let log_dir = root.join(".claude/projects/-tmp-project-a");
        fs::create_dir_all(&log_dir).unwrap();
        fs::write(
            log_dir.join("session.jsonl"),
            r#"{"type":"user","sessionId":"s1","cwd":"/tmp/project-a","timestamp":"2026-05-17T00:00:00Z","message":{"content":"hello"}}
{"type":"assistant","sessionId":"s1","cwd":"/tmp/project-a","timestamp":"2026-05-17T00:01:00Z","uuid":"a1","message":{"model":"claude-sonnet","usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":10}}}
"#,
        )
        .unwrap();

        let snapshot = load_project_history_without_store(
            AIHistoryProjectRequest {
                id: "project-1".to_string(),
                name: "Project".to_string(),
                path: project_path.to_string(),
            },
            &root,
            &mut |_, _| {},
        );

        assert_eq!(snapshot.project_summary.project_total_tokens, 150);
        assert_eq!(snapshot.project_summary.project_cached_input_tokens, 10);
        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.sessions[0].request_count, 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn codex_uses_state_database_before_recursive_scan() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = root.join("project-a").to_string_lossy().to_string();
        let codex_dir = root.join(".codex");
        fs::create_dir_all(codex_dir.join("sessions")).unwrap();
        let rollout_path = codex_dir.join("sessions").join("rollout.jsonl");
        fs::write(
            &rollout_path,
            format!(
                r#"{{"timestamp":"2026-05-17T00:00:00Z","type":"session_meta","payload":{{"cwd":"{}","id":"s1"}}}}"#,
                project_path
            ),
        )
        .unwrap();
        let database_path = codex_dir.join("state_5.sqlite");
        let conn = Connection::open(&database_path).unwrap();
        conn.execute(
            "CREATE TABLE threads (rollout_path TEXT, cwd TEXT, updated_at REAL);",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO threads (rollout_path, cwd, updated_at) VALUES (?1, ?2, 2);",
            rusqlite::params![
                rollout_path.to_string_lossy().to_string(),
                project_path.clone()
            ],
        )
        .unwrap();

        let files = codex_session_paths(&project_path, &root);

        assert_eq!(files, vec![rollout_path]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn matches_windows_extended_paths_without_matching_project_children() {
        assert!(paths_equivalent(
            Some(r"\\?\F:\codux-tauri"),
            r"F:\codux-tauri"
        ));
        assert!(!paths_equivalent(
            Some(r"F:\codux-tauri-other"),
            r"F:\codux-tauri"
        ));
        assert!(!paths_equivalent(
            Some(r"F:\codux-tauri\.codux\worktrees\task-a"),
            r"F:\codux-tauri"
        ));
    }

    #[test]
    fn indexes_opencode_sqlite_history() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = root.join("project-a").to_string_lossy().to_string();
        let db_dir = root.join(".local/share/opencode");
        fs::create_dir_all(&db_dir).unwrap();
        let database_path = db_dir.join("opencode.db");
        let conn = Connection::open(&database_path).unwrap();
        conn.execute(
            "CREATE TABLE session (id TEXT PRIMARY KEY, title TEXT, time_archived REAL);",
            [],
        )
        .unwrap();
        conn.execute(
            "CREATE TABLE message (session_id TEXT, data TEXT, time_created REAL);",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO session (id, title, time_archived) VALUES ('ses_1', 'OpenCode Session', NULL);",
            [],
        )
        .unwrap();
        let user_payload = serde_json::json!({
            "role": "user",
            "time": { "created": "2026-05-17T00:00:00Z" },
            "path": { "root": project_path },
            "modelID": "model-a"
        });
        let assistant_payload = serde_json::json!({
            "role": "assistant",
            "time": { "created": "2026-05-17T00:01:00Z" },
            "path": { "root": project_path },
            "modelID": "model-a",
            "tokens": {
                "input": 10,
                "output": 5,
                "reasoning": 2,
                "cache": { "read": 3 }
            }
        });
        conn.execute(
            "INSERT INTO message (session_id, data, time_created) VALUES ('ses_1', ?1, 1);",
            [user_payload.to_string()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO message (session_id, data, time_created) VALUES ('ses_1', ?1, 2);",
            [assistant_payload.to_string()],
        )
        .unwrap();

        let snapshot = load_project_history_without_store(
            AIHistoryProjectRequest {
                id: "project-1".to_string(),
                name: "Project".to_string(),
                path: project_path,
            },
            &root,
            &mut |_, _| {},
        );

        assert_eq!(snapshot.project_summary.project_total_tokens, 17);
        assert_eq!(snapshot.project_summary.project_cached_input_tokens, 3);
        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.sessions[0].last_tool.as_deref(), Some("opencode"));
        assert_eq!(snapshot.sessions[0].request_count, 1);
        assert_eq!(snapshot.tool_breakdown[0].key, "opencode");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parses_kiro_history_json() {
        let root = std::env::temp_dir().join(format!("codux-history-test-{}", Uuid::new_v4()));
        let project_path = root.join("project-a").to_string_lossy().to_string();
        let session_dir = root.join(".kiro/sessions/cli");
        fs::create_dir_all(&session_dir).unwrap();
        let file_path = session_dir.join("session-abc.json");
        fs::write(
            &file_path,
            serde_json::json!({
                "sessionId": "session-abc",
                "projectPath": project_path,
                "model": "kiro-1",
                "title": "Kiro Session",
                "updatedAt": 1000,
                "messages": [
                    { "role": "user", "timestamp": "2026-05-17T00:00:00Z" },
                    { "role": "assistant", "timestamp": "2026-05-17T00:01:00Z", "content": "hello from kiro" }
                ],
                "usage": { "input_tokens": 12, "output_tokens": 8, "cache": { "read": 4 } }
            })
            .to_string(),
        )
        .unwrap();

        let snapshot = load_project_history_without_store(
            AIHistoryProjectRequest {
                id: "project-1".to_string(),
                name: "Project".to_string(),
                path: project_path,
            },
            &root,
            &mut |_, _| {},
        );

        assert_eq!(snapshot.sessions.len(), 1);
        assert_eq!(snapshot.sessions[0].last_tool.as_deref(), Some("kiro"));
        assert_eq!(snapshot.sessions[0].request_count, 1);
        assert_eq!(
            snapshot
                .tool_breakdown
                .iter()
                .any(|item| item.key == "kiro"),
            true
        );
        let _ = fs::remove_dir_all(root);
    }
}
