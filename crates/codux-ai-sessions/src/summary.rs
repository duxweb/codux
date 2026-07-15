use super::helpers::local_today_start_seconds;
use super::queries::{
    load_global_project_totals, load_global_recent_sessions, load_global_recent_time_buckets,
    load_global_today_cached_tokens, load_global_today_time_buckets, load_project_aggregates,
    load_sessions, load_today_tokens,
};
use super::{
    AIGlobalHistoryRangeSummary, AIGlobalHistorySummary, AIHistoryService, AIHistorySummary,
    AIProjectUsageSummary,
};
use rusqlite::{Connection, OptionalExtension};

impl AIHistoryService {
    pub fn project_summary(&self, project_path: &str) -> AIHistorySummary {
        if !self.database_path.is_file() {
            return AIHistorySummary {
                error: Some("ai-usage.sqlite3 not found".to_string()),
                ..Default::default()
            };
        }

        let conn = match Connection::open(&self.database_path) {
            Ok(conn) => conn,
            Err(error) => {
                return AIHistorySummary {
                    error: Some(error.to_string()),
                    ..Default::default()
                };
            }
        };

        if can_read_normalized_snapshot(&conn) {
            let project = conn
                .query_row(
                    r#"
                    SELECT project_id, project_name, indexed_at
                    FROM ai_history_project_index_state
                    WHERE project_path = ?1
                    LIMIT 1
                    "#,
                    [project_path],
                    |row| {
                        Ok(codux_ai_history::normalized::AIHistoryProjectRequest {
                            id: row.get(0)?,
                            name: row.get(1)?,
                            path: project_path.to_string(),
                        })
                    },
                )
                .optional();
            if let Ok(Some(project)) = project {
                return match codux_ai_history::normalized::load_indexed_project_history_at(
                    self.database_path.clone(),
                    project,
                ) {
                    Ok(Some(snapshot)) => project_summary_from_normalized_snapshot(snapshot),
                    Ok(None) => AIHistorySummary::default(),
                    Err(error) => AIHistorySummary {
                        error: Some(error.to_string()),
                        ..Default::default()
                    },
                };
            }
        }

        let indexed_at = conn
            .query_row(
                "SELECT indexed_at FROM ai_history_project_index_state WHERE project_path = ?1",
                [project_path],
                |row| row.get::<_, f64>(0),
            )
            .optional()
            .unwrap_or(None);
        let sessions = match load_sessions(&conn, project_path) {
            Ok(sessions) => sessions,
            Err(error) => {
                return AIHistorySummary {
                    indexed: indexed_at.is_some(),
                    indexed_at,
                    error: Some(error),
                    ..Default::default()
                };
            }
        };
        let (project_total_tokens, project_cached_input_tokens) =
            sessions.iter().fold((0, 0), |(total, cached), session| {
                (
                    total + session.total_tokens,
                    cached + session.cached_input_tokens,
                )
            });
        let today_start = local_today_start_seconds();
        let (today_total_tokens, today_cached_input_tokens) =
            load_today_tokens(&conn, project_path, today_start).unwrap_or((0, 0));
        let aggregates = load_project_aggregates(&conn, project_path, today_start)
            .unwrap_or_else(|_| Default::default());

        AIHistorySummary {
            indexed: indexed_at.is_some(),
            indexed_at,
            is_loading: false,
            queued: false,
            progress: None,
            detail: "idle".to_string(),
            project_total_tokens,
            project_cached_input_tokens,
            today_total_tokens,
            today_cached_input_tokens,
            session_count: sessions.len(),
            sessions,
            heatmap: aggregates.heatmap,
            today_time_buckets: aggregates.today_time_buckets,
            tool_breakdown: aggregates.tool_breakdown,
            model_breakdown: aggregates.model_breakdown,
            error: None,
        }
    }

    pub fn global_summary(&self) -> AIGlobalHistorySummary {
        if !self.database_path.is_file() {
            return AIGlobalHistorySummary {
                error: Some("ai-usage.sqlite3 not found".to_string()),
                ..Default::default()
            };
        }

        let conn = match Connection::open(&self.database_path) {
            Ok(conn) => conn,
            Err(error) => {
                return AIGlobalHistorySummary {
                    error: Some(error.to_string()),
                    ..Default::default()
                };
            }
        };

        if can_read_normalized_snapshot(&conn)
            && let Ok(Some(snapshot)) =
                codux_ai_history::normalized::load_all_indexed_global_history_at(
                    self.database_path.clone(),
                )
        {
            return global_summary_from_normalized_snapshot(snapshot);
        }

        let today_start = local_today_start_seconds();
        let project_totals = match load_global_project_totals(&conn, today_start) {
            Ok(projects) => projects,
            Err(error) => {
                return AIGlobalHistorySummary {
                    error: Some(error),
                    ..Default::default()
                };
            }
        };
        let recent_sessions = load_global_recent_sessions(&conn).unwrap_or_default();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs_f64())
            .unwrap_or(0.0);
        let recent_start = ((now / 1800.0).floor() + 1.0) * 1800.0 - 48.0 * 3600.0;
        let (total_tokens, cached_input_tokens, today_total_tokens) =
            project_totals.iter().fold((0, 0, 0), |acc, project| {
                (
                    acc.0 + project.total_tokens,
                    acc.1 + project.cached_input_tokens,
                    acc.2 + project.today_total_tokens,
                )
            });
        let session_count = project_totals
            .iter()
            .map(|project| project.session_count)
            .sum();
        let today_cached_input_tokens =
            load_global_today_cached_tokens(&conn, today_start).unwrap_or(0);

        let indexed_snapshot = can_read_normalized_snapshot(&conn)
            .then(|| {
                codux_ai_history::normalized::load_all_indexed_global_history_at(
                    self.database_path.clone(),
                )
                .ok()
                .flatten()
            })
            .flatten();
        let (
            heatmap,
            today_time_buckets,
            recent_time_buckets,
            tool_breakdown,
            model_breakdown,
            range_summaries,
        ) = indexed_snapshot
            .map(|snapshot| {
                (
                    snapshot.heatmap,
                    snapshot.today_time_buckets,
                    snapshot.recent_time_buckets,
                    snapshot.tool_breakdown,
                    snapshot.model_breakdown,
                    snapshot
                        .range_summaries
                        .into_iter()
                        .map(normalized_range_summary_to_summary)
                        .collect(),
                )
            })
            .unwrap_or_else(|| {
                (
                    Vec::new(),
                    load_global_recent_time_buckets(&conn, recent_start).unwrap_or_default(),
                    load_global_today_time_buckets(&conn, today_start).unwrap_or_default(),
                    Vec::new(),
                    Vec::new(),
                    Vec::new(),
                )
            });

        AIGlobalHistorySummary {
            indexed_project_count: project_totals.len(),
            session_count,
            total_tokens,
            cached_input_tokens,
            today_total_tokens,
            today_cached_input_tokens,
            project_totals,
            heatmap,
            today_time_buckets,
            recent_time_buckets,
            tool_breakdown,
            model_breakdown,
            range_summaries,
            recent_sessions,
            error: None,
        }
    }
}

pub fn global_summary_from_normalized_snapshot(
    snapshot: codux_ai_history::normalized::AIGlobalHistorySnapshot,
) -> AIGlobalHistorySummary {
    AIGlobalHistorySummary {
        indexed_project_count: snapshot.project_count,
        session_count: snapshot.sessions.len(),
        total_tokens: snapshot.total_tokens,
        cached_input_tokens: snapshot.cached_input_tokens,
        today_total_tokens: snapshot.today_total_tokens,
        today_cached_input_tokens: snapshot.today_cached_input_tokens,
        project_totals: snapshot
            .project_totals
            .into_iter()
            .map(normalized_project_total_to_summary)
            .collect(),
        heatmap: snapshot.heatmap,
        today_time_buckets: snapshot.today_time_buckets,
        recent_time_buckets: snapshot.recent_time_buckets,
        tool_breakdown: snapshot.tool_breakdown,
        model_breakdown: snapshot.model_breakdown,
        range_summaries: snapshot
            .range_summaries
            .into_iter()
            .map(normalized_range_summary_to_summary)
            .collect(),
        recent_sessions: snapshot
            .sessions
            .into_iter()
            .take(80)
            .map(normalized_session_to_summary)
            .collect(),
        error: None,
    }
}

pub fn project_summary_from_normalized_snapshot(
    snapshot: codux_ai_history::normalized::AIHistorySnapshot,
) -> AIHistorySummary {
    AIHistorySummary {
        indexed: true,
        indexed_at: Some(snapshot.indexed_at),
        is_loading: false,
        queued: false,
        progress: Some(1.0),
        detail: "completed".to_string(),
        project_total_tokens: snapshot.project_summary.project_total_tokens,
        project_cached_input_tokens: snapshot.project_summary.project_cached_input_tokens,
        today_total_tokens: snapshot.project_summary.today_total_tokens,
        today_cached_input_tokens: snapshot.project_summary.today_cached_input_tokens,
        session_count: snapshot.sessions.len(),
        sessions: snapshot
            .sessions
            .into_iter()
            .map(normalized_session_to_summary)
            .collect(),
        heatmap: snapshot.heatmap,
        today_time_buckets: snapshot.today_time_buckets,
        tool_breakdown: snapshot.tool_breakdown,
        model_breakdown: snapshot.model_breakdown,
        error: None,
    }
}

fn can_read_normalized_snapshot(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT value FROM ai_history_meta WHERE key = 'normalized_history_schema_version' LIMIT 1",
        [],
        |row| row.get::<_, String>(0),
    )
    .is_ok()
}

fn normalized_project_total_to_summary(
    project: codux_ai_history::normalized::AIProjectUsageTotal,
) -> AIProjectUsageSummary {
    AIProjectUsageSummary {
        project_id: project.project_id,
        project_path: project.project_path,
        project_name: project.project_name,
        session_count: project.session_count,
        input_tokens: project.input_tokens,
        output_tokens: project.output_tokens,
        total_tokens: project.total_tokens,
        cached_input_tokens: project.cached_input_tokens,
        request_count: project.request_count,
        active_duration_seconds: project.active_duration_seconds,
        today_total_tokens: project.today_total_tokens,
        today_cached_input_tokens: project.today_cached_input_tokens,
    }
}

fn normalized_range_summary_to_summary(
    summary: codux_ai_history::normalized::AIGlobalHistoryRangeSummary,
) -> AIGlobalHistoryRangeSummary {
    AIGlobalHistoryRangeSummary {
        key: summary.key,
        input_tokens: summary.input_tokens,
        output_tokens: summary.output_tokens,
        total_tokens: summary.total_tokens,
        cached_input_tokens: summary.cached_input_tokens,
        request_count: summary.request_count,
        session_count: summary.session_count,
        active_duration_seconds: summary.active_duration_seconds,
        sessions: summary
            .sessions
            .into_iter()
            .map(normalized_session_to_summary)
            .collect(),
        project_totals: summary
            .project_totals
            .into_iter()
            .map(normalized_project_total_to_summary)
            .collect(),
        tool_breakdown: summary.tool_breakdown,
        model_breakdown: summary.model_breakdown,
    }
}

fn normalized_session_to_summary(
    session: codux_ai_history::normalized::AISessionSummary,
) -> super::AISessionSummary {
    super::AISessionSummary {
        id: session.session_id.clone(),
        session_key: session
            .external_session_id
            .clone()
            .unwrap_or_else(|| session.session_id.clone()),
        external_session_id: session.external_session_id,
        title: session.session_title,
        source: session.last_tool.unwrap_or_else(|| "ai".to_string()),
        project_name: Some(session.project_name),
        project_path: Some(session.project_path),
        last_model: session.last_model,
        last_seen_at: session.last_seen_at,
        input_tokens: session.total_input_tokens,
        output_tokens: session.total_output_tokens,
        total_tokens: session.total_tokens,
        cached_input_tokens: session.cached_input_tokens,
        request_count: session.request_count,
        active_duration_seconds: session.active_duration_seconds,
        usage_amounts: session
            .usage_amounts
            .into_iter()
            .map(|amount| super::AIUsageAmount {
                unit: amount.unit,
                value: amount.value,
            })
            .collect(),
    }
}
