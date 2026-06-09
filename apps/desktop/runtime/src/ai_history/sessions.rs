use super::helpers::{matching_session_keys, max_option, min_option};
use super::queries::{load_file_usage, load_session_detail_links, load_session_links};
use super::types::SessionLink;
use super::{AIHistoryService, AIHistorySummary, AISessionDetail, AISessionFileSummary};
use rusqlite::params;
use std::collections::HashMap;

impl AIHistoryService {
    pub fn rename_project_session(
        &self,
        project_path: &str,
        session_id: &str,
        title: &str,
    ) -> Result<AIHistorySummary, String> {
        let title = title.trim();
        if title.is_empty() {
            return Err("Session title cannot be empty.".to_string());
        }
        let mut conn = self.open_connection()?;
        let matched = matching_session_keys(&load_session_links(&conn, project_path)?, session_id);
        if matched.is_empty() {
            return Err("Session not found.".to_string());
        }
        let tx = conn.transaction().map_err(|error| error.to_string())?;
        for (source, session_key) in matched {
            tx.execute(
                r#"
                UPDATE ai_history_file_session_link
                SET session_title = ?1
                WHERE project_path = ?2 AND source = ?3 AND session_key = ?4
                "#,
                params![title, project_path, source, session_key],
            )
            .map_err(|error| error.to_string())?;
        }
        tx.commit().map_err(|error| error.to_string())?;
        Ok(self.project_summary(project_path))
    }

    pub fn remove_project_session(
        &self,
        project_path: &str,
        session_id: &str,
    ) -> Result<AIHistorySummary, String> {
        let mut conn = self.open_connection()?;
        let matched = matching_session_keys(&load_session_links(&conn, project_path)?, session_id);
        if matched.is_empty() {
            return Err("Session not found.".to_string());
        }
        let tx = conn.transaction().map_err(|error| error.to_string())?;
        for (source, session_key) in matched {
            tx.execute(
                r#"
                DELETE FROM ai_history_file_usage_bucket
                WHERE project_path = ?1 AND source = ?2 AND session_key = ?3
                "#,
                params![project_path, source, session_key],
            )
            .map_err(|error| error.to_string())?;
            tx.execute(
                r#"
                DELETE FROM ai_history_file_session_link
                WHERE project_path = ?1 AND source = ?2 AND session_key = ?3
                "#,
                params![project_path, source, session_key],
            )
            .map_err(|error| error.to_string())?;
        }
        tx.commit().map_err(|error| error.to_string())?;
        Ok(self.project_summary(project_path))
    }

    pub fn project_session_detail(
        &self,
        project_path: &str,
        session_id: &str,
    ) -> Result<AISessionDetail, String> {
        let conn = self.open_connection()?;
        let links = load_session_detail_links(&conn, project_path)?;
        let simple_links = links
            .iter()
            .map(|link| SessionLink {
                source: link.source.clone(),
                session_key: link.session_key.clone(),
                external_session_id: link.external_session_id.clone(),
            })
            .collect::<Vec<_>>();
        let matched = matching_session_keys(&simple_links, session_id);
        if matched.is_empty() {
            return Err("Session not found.".to_string());
        }

        let mut detail = AISessionDetail {
            id: session_id.to_string(),
            ..Default::default()
        };
        let mut files = Vec::new();
        let mut active_duration_by_key = HashMap::<(String, String), i64>::new();

        for link in links.into_iter().filter(|link| {
            matched
                .iter()
                .any(|(source, key)| source == &link.source && key == &link.session_key)
        }) {
            if detail.title.is_empty() || link.last_seen_at > detail.last_seen_at {
                detail.title = link.title.clone();
                detail.source = link.source.clone();
                detail.session_key = link.session_key.clone();
                detail.external_session_id = link.external_session_id.clone();
            }
            detail.first_seen_at = min_option(detail.first_seen_at, link.first_seen_at);
            detail.last_seen_at = max_option(detail.last_seen_at, link.last_seen_at);
            active_duration_by_key
                .entry((link.source.clone(), link.session_key.clone()))
                .or_insert(link.active_duration_seconds);

            let (total_tokens, cached_input_tokens, request_count) = load_file_usage(
                &conn,
                project_path,
                &link.source,
                &link.file_path,
                &link.session_key,
            )
            .unwrap_or((0, 0, 0));
            detail.total_tokens += total_tokens;
            detail.cached_input_tokens += cached_input_tokens;
            detail.request_count += request_count;
            files.push(AISessionFileSummary {
                file_path: link.file_path,
                model: link.last_model.unwrap_or_else(|| "unknown".to_string()),
                first_seen_at: link.first_seen_at,
                last_seen_at: link.last_seen_at,
                total_tokens,
                cached_input_tokens,
                request_count,
            });
        }

        detail.active_duration_seconds = active_duration_by_key.values().copied().sum();
        files.sort_by(|a, b| {
            b.last_seen_at
                .partial_cmp(&a.last_seen_at)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        detail.files = files;
        if detail.title.is_empty() {
            detail.title = "Untitled session".to_string();
        }
        Ok(detail)
    }
}
