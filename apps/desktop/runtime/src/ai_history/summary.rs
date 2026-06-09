use super::helpers::local_today_start_seconds;
use super::queries::{
    load_global_project_totals, load_global_recent_sessions, load_global_today_cached_tokens,
    load_project_aggregates, load_sessions, load_today_tokens,
};
use super::{AIGlobalHistorySummary, AIHistoryService, AIHistorySummary};
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

        AIGlobalHistorySummary {
            indexed_project_count: project_totals.len(),
            session_count,
            total_tokens,
            cached_input_tokens,
            today_total_tokens,
            today_cached_input_tokens: load_global_today_cached_tokens(&conn, today_start)
                .unwrap_or(0),
            project_totals,
            recent_sessions,
            error: None,
        }
    }
}
