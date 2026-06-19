impl MemoryService {
    pub fn retry_failed_extraction_task(
        &self,
        task_id: &str,
    ) -> Result<MemoryExtractionStatusSnapshot, String> {
        let task_id = task_id.trim();
        if task_id.is_empty() {
            return Err("Memory extraction task id is empty.".to_string());
        }
        self.ensure_queue_schema()?;
        let conn = self.open_connection()?;
        let changed = conn
            .execute(
                r#"
                UPDATE memory_extraction_queue
                SET status = 'pending',
                    error = NULL,
                    enqueued_at = ?2
                WHERE id = ?1
                  AND status = 'failed';
                "#,
                params![task_id, now_seconds()],
            )
            .map_err(|error| error.to_string())?;
        if changed == 0 {
            return Err("Failed memory extraction task not found.".to_string());
        }
        self.extraction_status_snapshot()
    }

    pub fn set_entry_status(
        &self,
        project_id: Option<&str>,
        entry_id: &str,
        status: &str,
    ) -> Result<MemorySummary, String> {
        let status = match status.trim() {
            "active" => "active",
            "archived" => "archived",
            _ => return Err("Unsupported memory status.".to_string()),
        };
        let entry_id = entry_id.trim();
        if entry_id.is_empty() {
            return Err("Memory entry id is empty.".to_string());
        }
        let conn = self.open_connection()?;
        let changed = conn
            .execute(
                r#"
                UPDATE memory_entries
                SET status = ?1,
                    archived_at = CASE WHEN ?1 = 'archived' THEN unixepoch('now') ELSE NULL END,
                    updated_at = unixepoch('now')
                WHERE id = ?2
                  AND (?3 IS NULL OR project_id = ?3 OR scope = 'user')
                "#,
                params![status, entry_id, project_id],
            )
            .map_err(|error| error.to_string())?;
        if changed == 0 {
            return Err("Memory entry not found.".to_string());
        }
        self.summary_from_conn(&conn, project_id, true)
    }

    pub fn manager_snapshot(
        &self,
        projects: &[MemoryProjectInfo],
        scope: &str,
        project_id: Option<&str>,
        tab: &str,
        limit: i64,
    ) -> MemoryManagerSnapshot {
        if !self.database_path.is_file() {
            return MemoryManagerSnapshot {
                error: Some("memory.sqlite3 not found".to_string()),
                ..Default::default()
            };
        }
        let conn = match Connection::open(&self.database_path) {
            Ok(conn) => conn,
            Err(error) => {
                return MemoryManagerSnapshot {
                    error: Some(error.to_string()),
                    ..Default::default()
                };
            }
        };
        match self.manager_snapshot_from_conn(&conn, projects, scope, project_id, tab, limit) {
            Ok(snapshot) => snapshot,
            Err(error) => MemoryManagerSnapshot {
                available: true,
                error: Some(error),
                ..Default::default()
            },
        }
    }

    pub fn delete_entry(
        &self,
        project_id: Option<&str>,
        entry_id: &str,
    ) -> Result<MemorySummary, String> {
        let entry_id = entry_id.trim();
        if entry_id.is_empty() {
            return Err("Memory entry id is empty.".to_string());
        }
        let conn = self.open_connection()?;
        let changed = conn
            .execute(
                r#"
                DELETE FROM memory_entries
                WHERE id = ?1
                  AND (?2 IS NULL OR project_id = ?2 OR scope = 'user')
                "#,
                params![entry_id, project_id],
            )
            .map_err(|error| error.to_string())?;
        if changed == 0 {
            return Err("Memory entry not found.".to_string());
        }
        self.summary_from_conn(&conn, project_id, true)
    }

    pub fn delete_summary(
        &self,
        project_id: Option<&str>,
        summary_id: &str,
    ) -> Result<MemorySummary, String> {
        let summary_id = summary_id.trim();
        if summary_id.is_empty() {
            return Err("Memory summary id is empty.".to_string());
        }
        let conn = self.open_connection()?;
        let changed = conn
            .execute(
                r#"
                DELETE FROM memory_summaries
                WHERE id = ?1
                  AND (?2 IS NULL OR project_id = ?2 OR scope = 'user')
                "#,
                params![summary_id, project_id],
            )
            .map_err(|error| error.to_string())?;
        if changed == 0 {
            return Err("Memory summary not found.".to_string());
        }
        let _ = conn.execute(
            "DELETE FROM memory_summary_versions WHERE summary_id = ?1",
            params![summary_id],
        );
        let _ = conn.execute(
            "UPDATE memory_entries SET merged_summary_id = NULL, updated_at = unixepoch('now') WHERE merged_summary_id = ?1",
            params![summary_id],
        );
        self.summary_from_conn(&conn, project_id, true)
    }

    pub fn delete_project_profile(&self, project_id: &str) -> Result<MemorySummary, String> {
        let project_id = project_id.trim();
        if project_id.is_empty() {
            return Err("Project id is empty.".to_string());
        }
        let conn = self.open_connection()?;
        let changed = conn
            .execute(
                "DELETE FROM memory_project_profiles WHERE project_id = ?1",
                params![project_id],
            )
            .map_err(|error| error.to_string())?;
        if changed == 0 {
            return Err("Memory project profile not found.".to_string());
        }
        self.summary_from_conn(&conn, Some(project_id), true)
    }

    fn manager_snapshot_from_conn(
        &self,
        conn: &Connection,
        projects: &[MemoryProjectInfo],
        scope: &str,
        project_id: Option<&str>,
        tab: &str,
        limit: i64,
    ) -> Result<MemoryManagerSnapshot, String> {
        let scope = normalize_scope(scope);
        let project_id = if scope == "project" { project_id } else { None };
        let limit = limit.clamp(1, 1000);
        let target_rows = manager_target_rows(conn, projects)?;
        let selected_target_title = selected_memory_target_title(&target_rows, scope, project_id);
        let current_overview = memory_scope_overview(conn, scope, project_id)?;
        let (entries, summaries, queued_extractions, failed_extractions) = match tab {
            "summary" => (
                Vec::new(),
                list_summaries_for_management(conn, scope, project_id)?,
                Vec::new(),
                Vec::new(),
            ),
            "failed" => (
                Vec::new(),
                Vec::new(),
                Vec::new(),
                self.failed_extraction_tasks(project_id, limit)?,
            ),
            "queue" => (
                Vec::new(),
                Vec::new(),
                self.active_extraction_tasks(project_id, limit)?,
                Vec::new(),
            ),
            "history" => (
                list_entries_for_management(
                    conn,
                    scope,
                    project_id,
                    None,
                    Some("archived"),
                    limit,
                )?,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            ),
            _ => (
                list_entries_for_management(conn, scope, project_id, None, Some("active"), limit)?,
                Vec::new(),
                Vec::new(),
                Vec::new(),
            ),
        };
        let project_profile = if scope == "project" {
            project_id.and_then(|id| current_project_profile(conn, id).ok().flatten())
        } else {
            None
        };
        Ok(MemoryManagerSnapshot {
            available: true,
            target_rows,
            selected_target_title,
            current_overview,
            project_profile,
            entries,
            summaries,
            queued_extractions,
            failed_extractions,
            extraction: MemoryExtractionSummary {
                queued: count_queue(conn, &["queued", "pending"])?,
                running: count_queue(conn, &["running"])?,
                failed: count_queue(conn, &["failed"])?,
                last_error: latest_failed_queue_error(conn)?,
            },
            error: None,
        })
    }
}
